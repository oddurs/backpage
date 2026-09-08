//! `backpage` renders a read-only dashboard frame.
//!
//! Today it writes the frame to stdout. Drawing it onto the macOS desktop
//! picture is not implemented yet — see the roadmap in `README.md`.

mod cli;
mod render;

use std::process::ExitCode;

use cli::{Command, parse};
use render::{Frame, Panel};

/// Usage text shown by `--help`.
const USAGE: &str = "\
backpage — turn the macOS desktop wallpaper into a read-only TUI dashboard

Usage:
  backpage [--width <n>] [--height <n>]
  backpage --help
  backpage --version

Options:
      --width <n>   Frame width in characters [default: 80]
      --height <n>  Frame height in lines [default: 24]
  -h, --help        Print this message
  -V, --version     Print the version
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
        Ok(Command::Render { width, height }) => {
            print!("{}", Frame::new(width, height).render(&panels()));
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("backpage: {err}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// The panels shown on the dashboard.
///
/// The contents are placeholders until the data sources are wired up.
fn panels() -> Vec<Panel> {
    vec![Panel::new(
        "backpage",
        vec![
            "no data sources configured".into(),
            "run `backpage --help` for usage".into(),
        ],
    )]
}
