//! Command-line argument parsing.

use std::fmt;

use crate::image::Rgb;

/// Default grid width when printing to stdout.
pub const DEFAULT_COLS: usize = 80;

/// Default grid height when printing to stdout.
pub const DEFAULT_ROWS: usize = 24;

/// Default monospace font. SF Mono ships with macOS as a plain TrueType file.
pub const DEFAULT_FONT: &str = "/System/Library/Fonts/SFNSMono.ttf";

/// Largest font size auto-fitting will choose, so a sparse sample does not
/// produce absurdly large type.
pub const MAX_AUTO_SIZE: f32 = 64.0;

/// Default gap between the frame and the edge of the picture, in pixels.
pub const DEFAULT_MARGIN: u32 = 96;

/// Default program consulted for a sample.
pub const DEFAULT_SOURCE: &str = "poptop";

/// How the dashboard should be produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Program run with `--once` to collect a sample.
    pub source: String,
    /// Path to a `.ttf` or `.otf` monospace font.
    pub font: String,
    /// Font size in pixels; `None` fits the type to the content.
    pub size: Option<f32>,
    /// Gap between the frame and the picture's edge, in pixels.
    pub margin: u32,
    /// Background colour.
    pub bg: Rgb,
    /// Foreground colour.
    pub fg: Rgb,
    /// Grid size, when overridden explicitly.
    pub grid: Option<(usize, usize)>,
    /// Picture size in pixels, when overridden explicitly.
    pub screen: Option<(u32, u32)>,
    /// Seconds between refreshes; `None` renders once and exits.
    pub interval: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: DEFAULT_SOURCE.to_string(),
            font: DEFAULT_FONT.to_string(),
            size: None,
            margin: DEFAULT_MARGIN,
            bg: Rgb(0x0c, 0x10, 0x16),
            fg: Rgb(0xc8, 0xd3, 0xdc),
            grid: None,
            screen: None,
            interval: None,
        }
    }
}

/// What the program was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Render the dashboard and set it as the desktop picture.
    Desktop(Box<Config>),
    /// Print the dashboard to stdout instead of touching the desktop.
    Stdout(Box<Config>),
    /// Print usage.
    Help,
    /// Print the version.
    Version,
}

/// Why a command line could not be understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A flag that does not exist.
    UnknownFlag(String),
    /// A flag that takes a value was given none.
    MissingValue(&'static str),
    /// A flag's value could not be read.
    InvalidValue {
        /// The flag whose value could not be read.
        flag: &'static str,
        /// The value as it was given.
        value: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag(flag) => write!(f, "unknown flag: {flag}"),
            Self::MissingValue(flag) => write!(f, "{flag} needs a value"),
            Self::InvalidValue { flag, value } => write!(f, "{flag} cannot take {value:?}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses arguments, which must not include the program name.
///
/// # Errors
///
/// Returns [`ParseError`] when a flag is unknown, missing its value, or given a
/// value that cannot be read.
#[allow(clippy::too_many_lines)] // A flat match over flags is clearer than a dispatch table.
pub fn parse<I>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut cfg = Config::default();
    let mut to_stdout = false;
    let mut cols = DEFAULT_COLS;
    let mut rows = DEFAULT_ROWS;
    let mut grid_set = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--stdout" => to_stdout = true,
            "--source" => cfg.source = text(&mut args, "--source")?,
            "--font" => cfg.font = text(&mut args, "--font")?,
            "--size" => cfg.size = Some(positive_float(&mut args, "--size")?),
            "--margin" => cfg.margin = number(&mut args, "--margin")?,
            "--bg" => cfg.bg = colour(&mut args, "--bg")?,
            "--fg" => cfg.fg = colour(&mut args, "--fg")?,
            "--interval" => cfg.interval = Some(u64::from(positive(&mut args, "--interval")?)),
            "--cols" => {
                cols = positive(&mut args, "--cols")? as usize;
                grid_set = true;
            }
            "--rows" => {
                rows = positive(&mut args, "--rows")? as usize;
                grid_set = true;
            }
            "--screen" => cfg.screen = Some(dimensions(&mut args, "--screen")?),
            other => return Err(ParseError::UnknownFlag(other.to_string())),
        }
    }

    if grid_set || to_stdout {
        cfg.grid = Some((cols, rows));
    }

    Ok(if to_stdout {
        Command::Stdout(Box::new(cfg))
    } else {
        Command::Desktop(Box::new(cfg))
    })
}

/// Reads the next argument as a string value for `flag`.
fn text<I>(args: &mut I, flag: &'static str) -> Result<String, ParseError>
where
    I: Iterator<Item = String>,
{
    args.next().ok_or(ParseError::MissingValue(flag))
}

/// Reads the next argument as an unsigned integer.
fn number<I>(args: &mut I, flag: &'static str) -> Result<u32, ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = text(args, flag)?;
    raw.parse()
        .map_err(|_| ParseError::InvalidValue { flag, value: raw })
}

/// Reads the next argument as an integer greater than zero.
fn positive<I>(args: &mut I, flag: &'static str) -> Result<u32, ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = text(args, flag)?;
    match raw.parse::<u32>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(ParseError::InvalidValue { flag, value: raw }),
    }
}

/// Reads the next argument as a finite float greater than zero.
fn positive_float<I>(args: &mut I, flag: &'static str) -> Result<f32, ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = text(args, flag)?;
    match raw.parse::<f32>() {
        Ok(n) if n > 0.0 && n.is_finite() => Ok(n),
        _ => Err(ParseError::InvalidValue { flag, value: raw }),
    }
}

/// Reads the next argument as `#rrggbb`.
fn colour<I>(args: &mut I, flag: &'static str) -> Result<Rgb, ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = text(args, flag)?;
    Rgb::parse(&raw).ok_or(ParseError::InvalidValue { flag, value: raw })
}

/// Reads the next argument as `WIDTHxHEIGHT`.
fn dimensions<I>(args: &mut I, flag: &'static str) -> Result<(u32, u32), ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = text(args, flag)?;
    let invalid = || ParseError::InvalidValue {
        flag,
        value: raw.clone(),
    };
    let (w, h) = raw.split_once('x').ok_or_else(invalid)?;
    let w: u32 = w.parse().map_err(|_| invalid())?;
    let h: u32 = h.parse().map_err(|_| invalid())?;
    if w == 0 || h == 0 {
        return Err(invalid());
    }
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Command, ParseError> {
        parse(args.iter().map(|s| (*s).to_string()))
    }

    fn config(args: &[&str]) -> Config {
        match parse_args(args).expect("should parse") {
            Command::Desktop(c) | Command::Stdout(c) => *c,
            other => panic!("expected a render command, got {other:?}"),
        }
    }

    #[test]
    fn no_arguments_paints_the_desktop() {
        assert!(matches!(parse_args(&[]), Ok(Command::Desktop(_))));
    }

    #[test]
    fn stdout_mode_is_opt_in() {
        assert!(matches!(parse_args(&["--stdout"]), Ok(Command::Stdout(_))));
    }

    #[test]
    fn stdout_mode_defaults_to_a_terminal_sized_grid() {
        assert_eq!(
            config(&["--stdout"]).grid,
            Some((DEFAULT_COLS, DEFAULT_ROWS))
        );
    }

    #[test]
    fn the_desktop_grid_is_computed_unless_it_is_given() {
        assert_eq!(config(&[]).grid, None);
        assert_eq!(config(&["--cols", "100"]).grid, Some((100, DEFAULT_ROWS)));
    }

    #[test]
    fn help_and_version_win_over_later_arguments() {
        assert_eq!(parse_args(&["--help", "--nonsense"]), Ok(Command::Help));
        assert_eq!(parse_args(&["-V", "--nonsense"]), Ok(Command::Version));
    }

    #[test]
    fn colours_are_parsed_from_hex() {
        let c = config(&["--bg", "#101010", "--fg", "abcdef"]);
        assert_eq!(c.bg, Rgb(0x10, 0x10, 0x10));
        assert_eq!(c.fg, Rgb(0xab, 0xcd, 0xef));
    }

    #[test]
    fn a_bad_colour_is_rejected() {
        assert_eq!(
            parse_args(&["--bg", "nope"]),
            Err(ParseError::InvalidValue {
                flag: "--bg",
                value: "nope".into()
            })
        );
    }

    #[test]
    fn the_screen_size_is_parsed_as_width_by_height() {
        assert_eq!(
            config(&["--screen", "1920x1080"]).screen,
            Some((1920, 1080))
        );
    }

    #[test]
    fn a_malformed_screen_size_is_rejected() {
        for bad in ["1920", "1920x", "x1080", "0x1080", "widexhigh"] {
            assert!(
                parse_args(&["--screen", bad]).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn an_interval_turns_it_into_a_loop() {
        assert_eq!(config(&["--interval", "5"]).interval, Some(5));
        assert_eq!(config(&[]).interval, None);
    }

    #[test]
    fn the_font_size_is_fitted_unless_it_is_given() {
        assert_eq!(config(&[]).size, None);
        assert_eq!(config(&["--size", "40"]).size, Some(40.0));
    }

    #[test]
    fn zero_is_not_a_valid_interval_or_size() {
        assert!(parse_args(&["--interval", "0"]).is_err());
        assert!(parse_args(&["--size", "0"]).is_err());
        assert!(parse_args(&["--cols", "0"]).is_err());
    }

    #[test]
    fn the_source_program_can_be_pointed_elsewhere() {
        assert_eq!(
            config(&["--source", "/opt/bin/poptop"]).source,
            "/opt/bin/poptop"
        );
        assert_eq!(config(&[]).source, DEFAULT_SOURCE);
    }

    #[test]
    fn unknown_flags_are_rejected() {
        assert_eq!(
            parse_args(&["--nope"]),
            Err(ParseError::UnknownFlag("--nope".into()))
        );
    }

    #[test]
    fn a_flag_without_its_value_is_rejected() {
        assert_eq!(
            parse_args(&["--font"]),
            Err(ParseError::MissingValue("--font"))
        );
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(
            ParseError::UnknownFlag("--x".into()).to_string(),
            "unknown flag: --x"
        );
        assert_eq!(
            ParseError::MissingValue("--font").to_string(),
            "--font needs a value"
        );
        assert_eq!(
            ParseError::InvalidValue {
                flag: "--bg",
                value: "x".into()
            }
            .to_string(),
            "--bg cannot take \"x\""
        );
    }
}
