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

/// A font loaded at a fixed pixel size.
pub struct Renderer<'a> {
    font: FontRef<'a>,
    scale: PxScale,
}

impl<'a> Renderer<'a> {
    /// Loads a font from its raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not a font this crate can parse.
    /// TrueType collections (`.ttc`) are not supported; use a `.ttf` or `.otf`.
    pub fn new(bytes: &'a [u8], size: f32) -> Result<Self, ab_glyph::InvalidFont> {
        Ok(Self {
            font: FontRef::try_from_slice(bytes)?,
            scale: PxScale::from(size),
        })
    }

    /// The same font at a different pixel size.
    #[must_use]
    pub fn resized(&self, size: f32) -> Self {
        Self {
            font: self.font.clone(),
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
        let scaled = self.font.as_scaled(self.scale);
        Metrics {
            cell_w: scaled.h_advance(self.font.glyph_id('M')),
            line_h: scaled.height() + scaled.line_gap(),
            ascent: scaled.ascent(),
        }
    }

    /// Draws `text` onto a `width` x `height` RGB buffer.
    ///
    /// Lines are laid out on the monospace grid from `origin`, the top-left of
    /// the text block in pixels.
    /// Anything that would fall outside the buffer is clipped.
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
        let (w, h) = (width as usize, height as usize);
        let mut buf = Vec::with_capacity(w * h * CHANNELS);
        for _ in 0..w * h {
            buf.extend_from_slice(&[bg.0, bg.1, bg.2]);
        }

        let m = self.metrics();
        let (origin_x, origin_y) = origin;

        for (row, line) in text.lines().enumerate() {
            let baseline = origin_y + m.ascent + row as f32 * m.line_h;
            if baseline - m.ascent > height as f32 {
                break;
            }
            for (col, ch) in line.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let x = origin_x + col as f32 * m.cell_w;
                if x > width as f32 {
                    break;
                }
                let glyph = self
                    .font
                    .glyph_id(ch)
                    .with_scale_and_position(self.scale, point(x, baseline));
                let Some(outline) = self.font.outline_glyph(glyph) else {
                    continue;
                };
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let px = bounds.min.x + gx as f32;
                    let py = bounds.min.y + gy as f32;
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
        Renderer::new(bytes, 24.0).expect("SF Mono should parse")
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
        assert!(Renderer::new(b"not a font", 24.0).is_err());
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
