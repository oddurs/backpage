//! Rasterising a text frame to a PNG.
//!
//! Rasterising is inherently float-to-integer work: glyph coverage and pixel
//! coordinates cross that boundary on every sample. Every conversion here is
//! bounds-checked before the value is used as an index, so the pedantic cast
//! lints are noise in this module specifically.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

/// Bytes per pixel in the buffers this module produces.
const CHANNELS: usize = 3;

/// Fraction of a cell a fallback glyph is scaled to occupy. Slightly under one
/// so a glyph does not bleed into the neighbouring column.
const FALLBACK_FILL: f32 = 0.95;

/// An 8-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#rrggbb` or `rrggbb`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self(byte(0)?, byte(2)?, byte(4)?))
    }

    /// Mixes towards `other` by `t`, clamped to `0.0..=1.0`.
    fn blend(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
}

/// One character cell with its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The character to draw.
    pub ch: char,
    /// Foreground colour.
    pub fg: Rgb,
    /// Background colour.
    pub bg: Rgb,
}

impl Cell {
    /// A blank cell in the given background.
    #[must_use]
    pub fn blank(bg: Rgb) -> Self {
        Self {
            ch: ' ',
            fg: bg,
            bg,
        }
    }
}

/// Cell metrics for a monospace font at a fixed size.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    /// Horizontal advance of one cell, in pixels.
    pub cell_w: f32,
    /// Distance between baselines, in pixels.
    pub line_h: f32,
    /// Distance from the top of a line to its baseline, in pixels.
    pub ascent: f32,
}

/// A stack of fonts at a fixed pixel size.
///
/// The first font decides the cell metrics. Later fonts are fallbacks, used
/// only for characters the earlier ones do not carry — on macOS the braille
/// block that terminal graphs are drawn with lives in no monospace font, so
/// drawing poptop's timeline needs one.
pub struct Renderer<'a> {
    fonts: Vec<FontRef<'a>>,
    /// Per-font pixel scale. Fallbacks are scaled so their advance matches the
    /// primary cell, since they are not designed to the same em.
    scales: Vec<PxScale>,
    scale: PxScale,
}

impl<'a> Renderer<'a> {
    /// Loads a primary font plus fallbacks, in order of preference.
    ///
    /// The first font decides the cell metrics; later ones are consulted only
    /// for characters the earlier ones lack. Each fallback is scaled so one of
    /// its glyphs occupies a cell, since it is not drawn to the same em.
    ///
    /// # Errors
    ///
    /// Returns an error if `fonts` is empty or an entry cannot be parsed.
    pub fn with_fallbacks(fonts: &[&'a [u8]], size: f32) -> Result<Self, ab_glyph::InvalidFont> {
        let parsed: Vec<FontRef<'a>> = fonts
            .iter()
            .map(|b| FontRef::try_from_slice(b))
            .collect::<Result<_, _>>()?;
        let primary = parsed.first().ok_or(ab_glyph::InvalidFont)?;
        let scale = PxScale::from(size);
        let cell_w = primary.as_scaled(scale).h_advance(primary.glyph_id('M'));

        // A fallback is normalised by the *drawn* size of a representative
        // glyph, not its advance: Apple Braille advances a full cell but draws
        // a small dot cluster, so matching advances leaves braille tiny.
        let scales = parsed
            .iter()
            .enumerate()
            .map(|(i, font)| {
                if i == 0 {
                    return scale;
                }
                let probe = ['\u{28ff}', 'M', '\u{2800}']
                    .into_iter()
                    .find(|&c| font.glyph_id(c).0 != 0)
                    .unwrap_or('M');
                let glyph = font
                    .glyph_id(probe)
                    .with_scale_and_position(scale, point(0.0, 0.0));
                let drawn = font
                    .outline_glyph(glyph)
                    .map_or(0.0, |o| o.px_bounds().max.x - o.px_bounds().min.x);
                if drawn > 0.0 {
                    PxScale::from(size * (cell_w * FALLBACK_FILL / drawn))
                } else {
                    scale
                }
            })
            .collect();

        Ok(Self {
            fonts: parsed,
            scales,
            scale,
        })
    }

    /// Index of the first font carrying `ch`, if any.
    fn font_for(&self, ch: char) -> Option<usize> {
        self.fonts.iter().position(|f| f.glyph_id(ch).0 != 0)
    }

    /// The same font stack at a different pixel size.
    #[must_use]
    pub fn resized(&self, size: f32) -> Self {
        let ratio = size / self.scale.y;
        Self {
            fonts: self.fonts.clone(),
            scales: self
                .scales
                .iter()
                .map(|s| PxScale::from(s.y * ratio))
                .collect(),
            scale: PxScale::from(size),
        }
    }

    /// The largest font size at which a `cols` x `rows` grid still fits.
    ///
    /// Cell metrics scale linearly with size, so this measures once at the
    /// current size and solves for the rest rather than searching.
    #[must_use]
    pub fn fit(&self, cols: usize, rows: usize, width: f32, height: f32) -> f32 {
        let m = self.metrics();
        let size = self.scale.y;
        let per_col = m.cell_w / size;
        let per_row = m.line_h / size;
        let by_width = width / (cols.max(1) as f32 * per_col);
        let by_height = height / (rows.max(1) as f32 * per_row);
        by_width.min(by_height).max(1.0)
    }

    /// Width and height in pixels of a `cols` x `rows` grid at this size.
    #[must_use]
    pub fn extent(&self, cols: usize, rows: usize) -> (f32, f32) {
        let m = self.metrics();
        (cols as f32 * m.cell_w, rows as f32 * m.line_h)
    }

    /// Cell metrics at the configured size.
    #[must_use]
    pub fn metrics(&self) -> Metrics {
        let primary = &self.fonts[0];
        let scaled = primary.as_scaled(self.scale);
        Metrics {
            cell_w: scaled.h_advance(primary.glyph_id('M')),
            line_h: scaled.height() + scaled.line_gap(),
            ascent: scaled.ascent(),
        }
    }

    /// Draws a grid of styled cells onto a `width` x `height` RGB buffer.
    ///
    /// Cell backgrounds are painted first, then glyphs, so a cell that differs
    /// from the page background reads as a block the way it does in a terminal.
    /// Anything outside the buffer is clipped.
    #[must_use]
    pub fn draw_cells(
        &self,
        cells: &[Cell],
        cols: usize,
        width: u32,
        height: u32,
        origin: (f32, f32),
        bg: Rgb,
    ) -> Vec<u8> {
        let (w, h) = (width as usize, height as usize);
        let mut buf = Vec::with_capacity(w * h * CHANNELS);
        for _ in 0..w * h {
            buf.extend_from_slice(&[bg.0, bg.1, bg.2]);
        }

        let m = self.metrics();
        let (ox, oy) = origin;
        let rows = if cols == 0 {
            0
        } else {
            cells.len().div_ceil(cols)
        };

        for row in 0..rows {
            let y0 = oy + row as f32 * m.line_h;
            for col in 0..cols {
                let Some(cell) = cells.get(row * cols + col) else {
                    continue;
                };
                if cell.bg != bg {
                    let x0 = ox + col as f32 * m.cell_w;
                    fill(&mut buf, (w, h), (x0, y0, m.cell_w, m.line_h), cell.bg);
                }
            }
        }

        for row in 0..rows {
            let baseline = oy + m.ascent + row as f32 * m.line_h;
            if baseline - m.ascent > height as f32 {
                break;
            }
            for col in 0..cols {
                let Some(cell) = cells.get(row * cols + col) else {
                    continue;
                };
                if cell.ch == ' ' || cell.ch == '\0' {
                    continue;
                }
                let x = ox + col as f32 * m.cell_w;
                if x > width as f32 {
                    break;
                }
                let Some(fi) = self.font_for(cell.ch) else {
                    continue;
                };
                let font = &self.fonts[fi];
                let glyph = font
                    .glyph_id(cell.ch)
                    .with_scale_and_position(self.scales[fi], point(x, baseline));
                let Some(outline) = font.outline_glyph(glyph) else {
                    continue;
                };
                let bounds = outline.px_bounds();
                // A fallback font is not drawn to the primary's baseline, so
                // centre its glyph in the cell instead. That is where a
                // terminal puts braille, and it is what the graphs assume.
                let dy = if fi == 0 {
                    0.0
                } else {
                    (oy + row as f32 * m.line_h + m.line_h / 2.0)
                        - f32::midpoint(bounds.min.y, bounds.max.y)
                };
                let fg = cell.fg;
                outline.draw(|gx, gy, coverage| {
                    let px = bounds.min.x + gx as f32;
                    let py = bounds.min.y + gy as f32 + dy;
                    if px < 0.0 || py < 0.0 || px >= width as f32 || py >= height as f32 {
                        return;
                    }
                    let idx = (py as usize * w + px as usize) * CHANNELS;
                    let current = Rgb(buf[idx], buf[idx + 1], buf[idx + 2]);
                    let mixed = current.blend(fg, coverage);
                    buf[idx] = mixed.0;
                    buf[idx + 1] = mixed.1;
                    buf[idx + 2] = mixed.2;
                });
            }
        }

        buf
    }

    /// Draws plain `text` in a single colour.
    #[must_use]
    pub fn draw(
        &self,
        text: &str,
        width: u32,
        height: u32,
        origin: (f32, f32),
        bg: Rgb,
        fg: Rgb,
    ) -> Vec<u8> {
        let lines: Vec<&str> = text.lines().collect();
        let cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let mut cells = Vec::with_capacity(cols * lines.len());
        for line in &lines {
            let mut n = 0;
            for ch in line.chars() {
                cells.push(Cell { ch, fg, bg });
                n += 1;
            }
            for _ in n..cols {
                cells.push(Cell::blank(bg));
            }
        }
        self.draw_cells(&cells, cols, width, height, origin, bg)
    }
}

/// Paints an axis-aligned rectangle, clipped to the buffer.
fn fill(buf: &mut [u8], size: (usize, usize), rect: (f32, f32, f32, f32), colour: Rgb) {
    let (buf_w, buf_h) = size;
    let (left, top, rect_w, rect_h) = rect;
    let x0 = left.max(0.0) as usize;
    let y0 = top.max(0.0) as usize;
    let x1 = ((left + rect_w).max(0.0) as usize).min(buf_w);
    let y1 = ((top + rect_h).max(0.0) as usize).min(buf_h);
    for py in y0..y1 {
        for px in x0..x1 {
            let idx = (py * buf_w + px) * CHANNELS;
            buf[idx] = colour.0;
            buf[idx + 1] = colour.1;
            buf[idx + 2] = colour.2;
        }
    }
}

/// Writes an RGB buffer to `path` as a PNG.
///
/// # Errors
///
/// Returns an error if the file cannot be written or the buffer length does not
/// match `width * height * 3`.
pub fn write_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> std::io::Result<()> {
    let expected = width as usize * height as usize * CHANNELS;
    if rgb.len() != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("buffer is {} bytes, expected {expected}", rgb.len()),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(rgb)
        .map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = "/System/Library/Fonts/SFNSMono.ttf";

    fn renderer(bytes: &[u8]) -> Renderer<'_> {
        Renderer::with_fallbacks(&[bytes], 24.0).expect("SF Mono should parse")
    }

    #[test]
    fn parses_hex_colours_with_and_without_a_hash() {
        assert_eq!(Rgb::parse("#0c1014"), Some(Rgb(0x0c, 0x10, 0x14)));
        assert_eq!(Rgb::parse("ffffff"), Some(Rgb(255, 255, 255)));
    }

    #[test]
    fn rejects_malformed_colours() {
        assert_eq!(Rgb::parse("#fff"), None);
        assert_eq!(Rgb::parse("gggggg"), None);
        assert_eq!(Rgb::parse(""), None);
    }

    #[test]
    fn blending_at_the_extremes_returns_the_endpoints() {
        let a = Rgb(0, 0, 0);
        let b = Rgb(255, 255, 255);
        assert_eq!(a.blend(b, 0.0), a);
        assert_eq!(a.blend(b, 1.0), b);
        assert_eq!(a.blend(b, 2.0), b, "coverage is clamped");
    }

    #[test]
    fn a_buffer_of_the_wrong_length_is_rejected() {
        let dir = std::env::temp_dir().join("backpage-test");
        let err = write_png(&dir.join("bad.png"), 2, 2, &[0; 3]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn font_loading_rejects_things_that_are_not_fonts() {
        assert!(Renderer::with_fallbacks(&[b"not a font"], 24.0).is_err());
    }

    // The remaining tests need the system font, which exists only on macOS.
    #[cfg(target_os = "macos")]
    mod macos {
        use super::*;

        fn font_bytes() -> Vec<u8> {
            std::fs::read(FONT).expect("SF Mono ships with macOS")
        }

        #[test]
        fn metrics_describe_a_non_degenerate_cell() {
            let bytes = font_bytes();
            let m = renderer(&bytes).metrics();
            assert!(m.cell_w > 0.0 && m.line_h > 0.0 && m.ascent > 0.0);
            assert!(m.line_h > m.ascent, "a line must be taller than its ascent");
        }

        #[test]
        fn drawing_produces_a_correctly_sized_buffer() {
            let bytes = font_bytes();
            let buf =
                renderer(&bytes).draw("hi", 100, 50, (4.0, 4.0), Rgb(0, 0, 0), Rgb(255, 255, 255));
            assert_eq!(buf.len(), 100 * 50 * CHANNELS);
        }

        #[test]
        fn drawing_text_actually_marks_the_buffer() {
            let bytes = font_bytes();
            let r = renderer(&bytes);
            let bg = Rgb(0, 0, 0);
            let blank = r.draw("", 200, 60, (4.0, 4.0), bg, Rgb(255, 255, 255));
            let drawn = r.draw("backpage", 200, 60, (4.0, 4.0), bg, Rgb(255, 255, 255));
            assert!(
                blank.iter().all(|&b| b == 0),
                "an empty frame stays background"
            );
            assert!(
                drawn.iter().any(|&b| b > 0),
                "text should light some pixels"
            );
        }

        #[test]
        fn text_beyond_the_buffer_is_clipped_rather_than_panicking() {
            let bytes = font_bytes();
            let r = renderer(&bytes);
            let long = "x".repeat(500);
            let many = vec![long.as_str(); 200].join("\n");
            let buf = r.draw(&many, 80, 40, (2.0, 2.0), Rgb(0, 0, 0), Rgb(255, 255, 255));
            assert_eq!(buf.len(), 80 * 40 * CHANNELS);
        }

        #[test]
        fn fitting_is_limited_by_whichever_axis_runs_out_first() {
            let bytes = font_bytes();
            let r = renderer(&bytes);
            let wide = r.fit(10, 10, 10_000.0, 100.0);
            let tall = r.fit(10, 10, 100.0, 10_000.0);
            assert!(wide < tall, "a short box must be limited by height");
        }

        #[test]
        fn a_fitted_size_actually_fits() {
            let bytes = font_bytes();
            let r = renderer(&bytes);
            let (cols, rows) = (100, 30);
            let (w, h) = (3264.0, 2042.0);
            let fitted = r.resized(r.fit(cols, rows, w, h));
            let (tw, th) = fitted.extent(cols, rows);
            assert!(
                tw <= w + 0.5 && th <= h + 0.5,
                "extent {tw}x{th} exceeds {w}x{h}"
            );
            assert!(tw > w * 0.9 || th > h * 0.9, "should fill one axis");
        }

        #[test]
        fn fitting_never_returns_a_degenerate_size() {
            let bytes = font_bytes();
            assert!(renderer(&bytes).fit(1000, 1000, 1.0, 1.0) >= 1.0);
        }

        #[test]
        fn a_written_png_is_readable_and_has_the_stated_size() {
            let bytes = font_bytes();
            let buf = renderer(&bytes).draw(
                "ok",
                64,
                32,
                (2.0, 2.0),
                Rgb(10, 10, 10),
                Rgb(200, 200, 200),
            );
            let path = std::env::temp_dir().join("backpage-test/frame.png");
            write_png(&path, 64, 32, &buf).expect("write");
            let decoder =
                png::Decoder::new(std::io::BufReader::new(File::open(&path).expect("open")));
            let reader = decoder.read_info().expect("header");
            let info = reader.info();
            assert_eq!((info.width, info.height), (64, 32));
        }
    }
}
