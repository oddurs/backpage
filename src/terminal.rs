//! Running a full-screen terminal program and reading back its screen.
//!
//! poptop's `--once` mode prints plain text. Its real interface — the timeline
//! graphs, the colours, the header — only exists on a terminal, so backpage
//! runs it in a pseudo-terminal and interprets what it draws.

use std::fmt;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::image::{Cell, Rgb};

/// Why a terminal session could not be started or read.
#[derive(Debug)]
pub enum TerminalError {
    /// The pseudo-terminal could not be opened.
    Pty(String),
    /// The program could not be started.
    Spawn {
        /// The program that was attempted.
        program: String,
        /// What went wrong.
        message: String,
    },
    /// The screen buffer was poisoned by a panicking reader thread.
    Poisoned,
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pty(m) => write!(f, "could not open a pseudo-terminal: {m}"),
            Self::Spawn { program, message } => write!(f, "could not run {program}: {message}"),
            Self::Poisoned => write!(f, "the terminal reader thread died"),
        }
    }
}

impl std::error::Error for TerminalError {}

/// How long to wait for a full-screen program to paint its first frame.
///
/// poptop samples before it draws anything, so a screen read immediately after
/// spawning is blank.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// Grace period after the first paint, so the figures on screen are real
/// rather than the zeroes of a program that has taken only one sample.
const SETTLE: Duration = Duration::from_millis(1500);

/// A program running on a pseudo-terminal, with its screen kept up to date.
pub struct Session {
    parser: Arc<Mutex<vt100::Parser>>,
    cols: usize,
    rows: usize,
    /// Kept alive so the child is not reaped while the session exists.
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Kept alive so the write side of the pty stays open, which keeps the
    /// child from seeing EOF on stdin and exiting immediately.
    _writer: Box<dyn std::io::Write + Send>,
}

impl Session {
    /// Starts `program` on a `cols` x `rows` pseudo-terminal.
    ///
    /// A reader thread feeds everything the program draws into a terminal
    /// parser, so [`Session::screen`] always reflects the latest frame.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError`] if the pty cannot be opened or the program
    /// cannot be started.
    pub fn spawn(program: &str, cols: u16, rows: u16) -> Result<Self, TerminalError> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TerminalError::Pty(e.to_string()))?;

        let mut cmd = CommandBuilder::new(program);
        // Truecolor, so poptop emits 38;2;R;G;B rather than falling back to a
        // 16-colour palette we would have to guess at.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TerminalError::Spawn {
                program: program.into(),
                message: e.to_string(),
            })?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::Pty(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerminalError::Pty(e.to_string()))?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let sink = Arc::clone(&parser);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut p) = sink.lock() {
                    p.process(&buf[..n]);
                }
            }
        });

        let session = Self {
            parser,
            cols: cols as usize,
            rows: rows as usize,
            _child: child,
            _writer: writer,
        };
        session.wait_for_first_frame();
        Ok(session)
    }

    /// Blocks until the program has drawn something, or the timeout passes.
    ///
    /// Returns whether anything was drawn; a caller that gets `false` will
    /// render a blank screen, which is better than failing outright since the
    /// next tick usually succeeds.
    fn wait_for_first_frame(&self) -> bool {
        let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(parser) = self.parser.lock() {
                let screen = parser.screen();
                let painted = (0..self.rows).any(|row| {
                    (0..self.cols).any(|col| {
                        let cell = screen.cell(
                            u16::try_from(row).unwrap_or(u16::MAX),
                            u16::try_from(col).unwrap_or(u16::MAX),
                        );
                        cell.is_some_and(|c| !c.contents().trim().is_empty())
                    })
                });
                if painted {
                    drop(parser);
                    std::thread::sleep(SETTLE);
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// The grid size this session was opened at.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// A snapshot of the screen as styled cells, row-major.
    ///
    /// `ink` and `paper` stand in wherever the program asked for the
    /// terminal's default foreground or background.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Poisoned`] if the reader thread panicked.
    pub fn screen(&self, ink: Rgb, paper: Rgb) -> Result<Vec<Cell>, TerminalError> {
        let parser = self.parser.lock().map_err(|_| TerminalError::Poisoned)?;
        let screen = parser.screen();
        Ok(cells_from(screen, self.cols, self.rows, ink, paper))
    }
}

/// Converts a parsed terminal screen into styled cells, row-major.
///
/// `ink` and `paper` stand in wherever the program asked for the terminal's
/// own default foreground or background.
fn cells_from(screen: &vt100::Screen, cols: usize, rows: usize, ink: Rgb, paper: Rgb) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let cell = screen.cell(
                u16::try_from(row).unwrap_or(u16::MAX),
                u16::try_from(col).unwrap_or(u16::MAX),
            );
            let Some(cell) = cell else {
                cells.push(Cell::blank(paper));
                continue;
            };
            let ch = cell.contents().chars().next().unwrap_or(' ');
            let mut fore = convert(cell.fgcolor(), ink);
            let mut back = convert(cell.bgcolor(), paper);
            if cell.inverse() {
                std::mem::swap(&mut fore, &mut back);
            }
            cells.push(Cell {
                ch,
                fg: fore,
                bg: back,
            });
        }
    }
    cells
}

/// Resolves a terminal colour to RGB, falling back to `default`.
fn convert(colour: vt100::Color, default: Rgb) -> Rgb {
    match colour {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => indexed(i),
        vt100::Color::Rgb(r, g, b) => Rgb(r, g, b),
    }
}

/// The xterm 256-colour palette.
fn indexed(i: u8) -> Rgb {
    const BASE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 49, 49),
        (13, 188, 121),
        (229, 229, 16),
        (36, 114, 200),
        (188, 63, 188),
        (17, 168, 205),
        (229, 229, 229),
        (102, 102, 102),
        (241, 76, 76),
        (35, 209, 139),
        (245, 245, 67),
        (59, 142, 234),
        (214, 112, 214),
        (41, 184, 219),
        (255, 255, 255),
    ];
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

    match i {
        0..=15 => {
            let (r, g, b) = BASE[i as usize];
            Rgb(r, g, b)
        }
        16..=231 => {
            let n = i - 16;
            Rgb(
                STEPS[(n / 36) as usize],
                STEPS[((n / 6) % 6) as usize],
                STEPS[(n % 6) as usize],
            )
        }
        232..=255 => {
            let level = 8 + (i - 232) * 10;
            Rgb(level, level, level)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_at(bytes: &[u8]) -> (Rgb, Rgb) {
        let mut parser = vt100::Parser::new(2, 10, 0);
        parser.process(bytes);
        let screen = parser.screen();
        let cell = screen.cell(0, 0).expect("cell 0,0 exists");
        (
            convert(cell.fgcolor(), Rgb(1, 1, 1)),
            convert(cell.bgcolor(), Rgb(2, 2, 2)),
        )
    }

    fn cells_of(bytes: &[u8], cols: usize, rows: usize) -> Vec<Cell> {
        let mut parser = vt100::Parser::new(
            u16::try_from(rows).unwrap(),
            u16::try_from(cols).unwrap(),
            0,
        );
        parser.process(bytes);
        cells_from(parser.screen(), cols, rows, Rgb(1, 1, 1), Rgb(2, 2, 2))
    }

    #[test]
    fn a_styled_cell_keeps_its_colours_rather_than_the_page_defaults() {
        // Regression: the conversion once computed the colours and then pushed
        // the defaults anyway, so every frame came out monochrome.
        let cells = cells_of(b"\x1b[38;2;255;102;102;49mX", 4, 1);
        assert_eq!(cells[0].ch, 'X');
        assert_eq!(
            cells[0].fg,
            Rgb(255, 102, 102),
            "the parsed colour must survive"
        );
    }

    #[test]
    fn an_unstyled_neighbour_still_uses_the_page_colours() {
        let cells = cells_of(b"\x1b[38;2;255;0;0mA\x1b[0mB", 4, 1);
        assert_eq!(cells[0].fg, Rgb(255, 0, 0));
        assert_eq!(cells[1].fg, Rgb(1, 1, 1));
    }

    #[test]
    fn inverse_video_swaps_the_two_colours() {
        let cells = cells_of(b"\x1b[7mX", 4, 1);
        assert_eq!((cells[0].fg, cells[0].bg), (Rgb(2, 2, 2), Rgb(1, 1, 1)));
    }

    #[test]
    fn the_grid_is_filled_even_where_nothing_was_drawn() {
        let cells = cells_of(b"X", 4, 2);
        assert_eq!(cells.len(), 8, "every cell of the grid is present");
        assert_eq!(cells[7].ch, ' ');
        assert_eq!(cells[7].bg, Rgb(2, 2, 2));
    }

    #[test]
    fn truecolor_sgr_reaches_the_cell() {
        // This is the form poptop emits: 38;2;R;G;B with a background reset.
        let (fg, _) = cell_at(b"\x1b[38;2;255;102;102;49mX");
        assert_eq!(fg, Rgb(255, 102, 102));
    }

    #[test]
    fn an_indexed_sgr_reaches_the_cell() {
        let (fg, _) = cell_at(b"\x1b[38;5;196mX");
        assert_eq!(fg, indexed(196));
    }

    #[test]
    fn an_unstyled_cell_uses_the_page_colours() {
        let (fg, bg) = cell_at(b"X");
        assert_eq!((fg, bg), (Rgb(1, 1, 1), Rgb(2, 2, 2)));
    }

    #[test]
    fn a_default_colour_uses_the_page_colour() {
        let fg = Rgb(1, 2, 3);
        assert_eq!(convert(vt100::Color::Default, fg), fg);
    }

    #[test]
    fn truecolor_passes_straight_through() {
        assert_eq!(
            convert(vt100::Color::Rgb(10, 20, 30), Rgb(0, 0, 0)),
            Rgb(10, 20, 30)
        );
    }

    #[test]
    fn the_palette_covers_its_three_ranges() {
        assert_eq!(indexed(0), Rgb(0, 0, 0));
        assert_eq!(indexed(15), Rgb(255, 255, 255));
        // 16 is the first cube entry, which is black again.
        assert_eq!(indexed(16), Rgb(0, 0, 0));
        // 231 is the last cube entry: full white.
        assert_eq!(indexed(231), Rgb(255, 255, 255));
        assert_eq!(indexed(232), Rgb(8, 8, 8));
        assert_eq!(indexed(255), Rgb(238, 238, 238));
    }

    #[test]
    fn the_colour_cube_walks_red_slowest_and_blue_fastest() {
        // 16 + 36*r + 6*g + b
        assert_eq!(indexed(16 + 36), Rgb(95, 0, 0));
        assert_eq!(indexed(16 + 6), Rgb(0, 95, 0));
        assert_eq!(indexed(16 + 1), Rgb(0, 0, 95));
    }

    #[test]
    fn every_palette_index_resolves() {
        for i in 0..=255u8 {
            let _ = indexed(i);
        }
    }
}
