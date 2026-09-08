//! Command-line argument parsing.

use std::fmt;

/// Default frame width when `--width` is not given.
pub const DEFAULT_WIDTH: usize = 80;

/// Default frame height when `--height` is not given.
pub const DEFAULT_HEIGHT: usize = 24;

/// What the program was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Render one frame to stdout.
    Render {
        /// Frame width in characters.
        width: usize,
        /// Frame height in lines.
        height: usize,
    },
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
    /// A flag's value was not a positive integer.
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
            Self::InvalidValue { flag, value } => {
                write!(f, "{flag} needs a positive integer, got {value:?}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses arguments, which must not include the program name.
///
/// # Errors
///
/// Returns [`ParseError`] when a flag is unknown, missing its value, or given a
/// value that is not a positive integer.
pub fn parse<I>(args: I) -> Result<Command, ParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut width = DEFAULT_WIDTH;
    let mut height = DEFAULT_HEIGHT;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--width" => width = value(&mut args, "--width")?,
            "--height" => height = value(&mut args, "--height")?,
            other => return Err(ParseError::UnknownFlag(other.to_string())),
        }
    }

    Ok(Command::Render { width, height })
}

/// Reads the next argument as a positive integer value for `flag`.
fn value<I>(args: &mut I, flag: &'static str) -> Result<usize, ParseError>
where
    I: Iterator<Item = String>,
{
    let raw = args.next().ok_or(ParseError::MissingValue(flag))?;
    match raw.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(ParseError::InvalidValue { flag, value: raw }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Command, ParseError> {
        parse(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn no_arguments_renders_at_the_default_size() {
        assert_eq!(
            parse_args(&[]),
            Ok(Command::Render {
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT
            })
        );
    }

    #[test]
    fn dimensions_can_be_overridden() {
        assert_eq!(
            parse_args(&["--width", "40", "--height", "10"]),
            Ok(Command::Render {
                width: 40,
                height: 10
            })
        );
    }

    #[test]
    fn help_and_version_win_over_later_arguments() {
        assert_eq!(parse_args(&["--help", "--nonsense"]), Ok(Command::Help));
        assert_eq!(parse_args(&["-V", "--nonsense"]), Ok(Command::Version));
    }

    #[test]
    fn short_flags_are_accepted() {
        assert_eq!(parse_args(&["-h"]), Ok(Command::Help));
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
            parse_args(&["--width"]),
            Err(ParseError::MissingValue("--width"))
        );
    }

    #[test]
    fn zero_and_non_numeric_dimensions_are_rejected() {
        assert_eq!(
            parse_args(&["--width", "0"]),
            Err(ParseError::InvalidValue {
                flag: "--width",
                value: "0".into()
            })
        );
        assert_eq!(
            parse_args(&["--height", "tall"]),
            Err(ParseError::InvalidValue {
                flag: "--height",
                value: "tall".into()
            })
        );
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(
            ParseError::UnknownFlag("--x".into()).to_string(),
            "unknown flag: --x"
        );
        assert_eq!(
            ParseError::MissingValue("--width").to_string(),
            "--width needs a value"
        );
        assert_eq!(
            ParseError::InvalidValue {
                flag: "--width",
                value: "x".into()
            }
            .to_string(),
            "--width needs a positive integer, got \"x\""
        );
    }
}
