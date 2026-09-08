//! `backpage` turns the macOS desktop picture into a read-only dashboard.
//!
//! It collects nothing itself: a sample comes from `poptop --once`, is composed
//! into a frame, rasterised to a PNG, and set as the desktop picture.

mod cli;
mod image;
mod render;
mod source;
mod wallpaper;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use cli::{Command, Config, parse};
use image::Renderer;
use render::{Frame, Panel};

/// Usage text shown by `--help`.
const USAGE: &str = "\
backpage — turn the macOS desktop picture into a read-only TUI dashboard

Usage:
  backpage [options]              paint the desktop once and exit
  backpage --interval 10          repaint every 10 seconds
  backpage --stdout               print the frame instead

Options:
      --source <prog>   program run with --once for a sample [default: poptop]
      --font <path>     .ttf or .otf monospace font
      --size <px>       font size [default: fitted to the content]
      --margin <px>     gap around the frame [default: 96]
      --bg <#rrggbb>    background colour
      --fg <#rrggbb>    foreground colour
      --screen <WxH>    picture size [default: the display's resolution]
      --cols <n>        grid width [default: whatever fits]
      --rows <n>        grid height [default: whatever fits]
      --interval <secs> repaint forever instead of once
      --stdout          print the frame rather than painting the desktop
  -h, --help            print this message
  -V, --version         print the version
";

fn main() -> ExitCode {
    match parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Stdout(cfg)) => run(&cfg, false),
        Ok(Command::Desktop(cfg)) => run(&cfg, true),
        Err(err) => {
            eprintln!("backpage: {err}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Renders once, or forever when an interval is configured.
fn run(cfg: &Config, to_desktop: bool) -> ExitCode {
    let font = match std::fs::read(&cfg.font) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("backpage: cannot read font {}: {err}", cfg.font);
            return ExitCode::FAILURE;
        }
    };
    let renderer = match Renderer::new(&font, cli::MAX_AUTO_SIZE) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("backpage: {} is not a usable font: {err}", cfg.font);
            eprintln!("  TrueType collections (.ttc) are not supported; use a .ttf or .otf");
            return ExitCode::FAILURE;
        }
    };

    if to_desktop {
        // Say what is being replaced, so the previous picture can be put back.
        if let Ok(previous) = wallpaper::current()
            && !previous.contains("Caches/backpage")
        {
            eprintln!("backpage: replacing desktop picture {previous}");
        }
    }

    let mut tick: u32 = 0;
    loop {
        if let Err(err) = once(cfg, &renderer, to_desktop, tick) {
            eprintln!("backpage: {err}");
            return ExitCode::FAILURE;
        }
        let Some(secs) = cfg.interval else {
            return ExitCode::SUCCESS;
        };
        tick = tick.wrapping_add(1);
        std::thread::sleep(Duration::from_secs(secs));
    }
}

/// Produces one frame and delivers it.
fn once(
    cfg: &Config,
    renderer: &Renderer<'_>,
    to_desktop: bool,
    tick: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let sample = source::read(&cfg.source)?;
    let panels = panels(&sample);

    if !to_desktop {
        let (cols, rows) = cfg.grid.unwrap_or((cli::DEFAULT_COLS, cli::DEFAULT_ROWS));
        print!("{}", Frame::new(cols, rows).render(&panels));
        return Ok(());
    }

    let (width, height) = match cfg.screen {
        Some(s) => s,
        None => wallpaper::screen_size()?,
    };
    let usable = |px: u32| f32::from(u16::try_from(px.saturating_sub(cfg.margin * 2)).unwrap_or(1));

    // The grid comes from the sample, not the screen: the type is then sized so
    // the content fills the picture, rather than padding a fixed grid with blanks.
    let (cols, rows) = cfg.grid.unwrap_or_else(|| natural_size(&panels));
    let size = cfg.size.unwrap_or_else(|| {
        renderer
            .fit(cols, rows, usable(width), usable(height))
            .min(cli::MAX_AUTO_SIZE)
    });
    let renderer = renderer.resized(size);

    let frame = Frame::new(cols, rows).render(&panels);
    let (text_w, text_h) = renderer.extent(cols, rows);
    let centre =
        |total: u32, text: f32| (f32::from(u16::try_from(total).unwrap_or(1)) - text) / 2.0;
    let origin = (centre(width, text_w), centre(height, text_h));

    let buf = renderer.draw(&frame, width, height, origin, cfg.bg, cfg.fg);
    let path = frame_path(tick);
    image::write_png(&path, width, height, &buf)?;
    wallpaper::set(&path)?;
    Ok(())
}

/// Where this tick's picture is written.
///
/// macOS keys the desktop picture on its path, so writing the same file again
/// does not always redraw. Alternating between two paths makes every refresh a
/// change of path, and keeps exactly two files on disk rather than a growing
/// pile.
fn frame_path(tick: u32) -> PathBuf {
    let base = std::env::var_os("HOME").map_or_else(
        || PathBuf::from("/tmp"),
        |home| Path::new(&home).join("Library/Caches/backpage"),
    );
    base.join(format!("frame-{}.png", tick % 2))
}

/// The grid a set of panels needs, with nothing padded or truncated.
fn natural_size(panels: &[Panel]) -> (usize, usize) {
    let cols = panels
        .iter()
        .map(Panel::natural_width)
        .max()
        .unwrap_or(cli::DEFAULT_COLS);
    let rows: usize = panels.iter().map(Panel::height).sum();
    (cols.max(1), rows.max(1))
}

/// The panels shown on the dashboard.
fn panels(sample: &source::Sample) -> Vec<Panel> {
    let mut panels = Vec::new();
    if !sample.stats.is_empty() {
        panels.push(Panel::new("system", sample.stats.clone()));
    }
    if !sample.processes.is_empty() {
        panels.push(Panel::new("processes", sample.processes.clone()));
    }
    if panels.is_empty() {
        panels.push(Panel::new("backpage", vec!["no sample available".into()]));
    }
    panels
}
