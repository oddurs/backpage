//! Setting the macOS desktop picture.
//!
//! Everything here shells out to `osascript`. The first run may raise a
//! permission prompt, because driving System Events counts as automation.

use std::fmt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why the desktop picture could not be read or written.
#[derive(Debug)]
pub enum WallpaperError {
    /// `osascript` could not be run.
    Spawn(std::io::Error),
    /// `osascript` ran but reported a problem.
    Script(String),
    /// The path could not be expressed as text for `AppleScript`.
    Path,
    /// `osascript` did not finish in time, which a locked display causes.
    TimedOut,
}

impl fmt::Display for WallpaperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not run osascript: {e}"),
            Self::Script(msg) => {
                write!(f, "osascript failed: {}", msg.trim())
            }
            Self::Path => write!(f, "path is not valid UTF-8"),
            Self::TimedOut => write!(
                f,
                "osascript timed out after {}s; is the display locked?",
                SCRIPT_TIMEOUT.as_secs()
            ),
        }
    }
}

impl std::error::Error for WallpaperError {}

/// Runs one `AppleScript` statement and returns its trimmed output.
/// How long to wait for `osascript` before giving up on it.
///
/// A locked or sleeping display leaves the `AppleEvent` pending, and without a
/// bound the process would sit there rather than trying again next tick.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(20);

fn osascript(script: &str) -> Result<String, WallpaperError> {
    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(WallpaperError::Spawn)?;

    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    loop {
        match child.try_wait().map_err(WallpaperError::Spawn)? {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WallpaperError::TimedOut);
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }

    let out = child.wait_with_output().map_err(WallpaperError::Spawn)?;
    if !out.status.success() {
        return Err(WallpaperError::Script(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The picture currently set on the desktop.
///
/// Reads `every desktop` rather than `current desktop`: the latter serves a
/// stale value after the picture is changed, reporting the previous path.
///
/// # Errors
///
/// Returns [`WallpaperError`] if `osascript` cannot be run or refuses.
pub fn current() -> Result<String, WallpaperError> {
    let all = osascript("tell application \"System Events\" to get picture of every desktop")?;
    Ok(first_desktop(&all).to_string())
}

/// Takes the first path from `osascript`'s comma-separated list form.
fn first_desktop(list: &str) -> &str {
    list.split(", ").next().unwrap_or(list).trim()
}

/// Sets `path` as the picture on every desktop.
///
/// # Errors
///
/// Returns [`WallpaperError`] if the path is not UTF-8, or `osascript` cannot be
/// run or refuses.
pub fn set(path: &Path) -> Result<(), WallpaperError> {
    let path = path.to_str().ok_or(WallpaperError::Path)?;
    if path.contains('"') || path.contains('\\') {
        // AppleScript string literals have no escaping worth trusting here, and
        // backpage controls the path it writes, so refuse rather than guess.
        return Err(WallpaperError::Path);
    }
    osascript(&format!(
        "tell application \"System Events\" to set picture of every desktop to \"{path}\""
    ))
    .map(|_| ())
}

/// The active display's pixel dimensions, read from `system_profiler`.
///
/// # Errors
///
/// Returns [`WallpaperError`] if `system_profiler` cannot be run, or no
/// resolution line can be parsed from its output.
pub fn screen_size() -> Result<(u32, u32), WallpaperError> {
    let out = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
        .map_err(WallpaperError::Spawn)?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_resolution(&text)
        .ok_or_else(|| WallpaperError::Script("no Resolution line in system_profiler".into()))
}

/// Extracts the first `Resolution: W x H` pair from `system_profiler` output.
fn parse_resolution(text: &str) -> Option<(u32, u32)> {
    for line in text.lines() {
        let Some((_, rest)) = line.split_once("Resolution:") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let w = parts.next()?.parse().ok()?;
        // The separator is an "x" between the two figures.
        if parts.next()? != "x" {
            continue;
        }
        let h = parts.next()?.parse().ok()?;
        return Some((w, h));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_retina_resolution_line() {
        let text = "      Display Type: Built-in Liquid Retina XDR Display\n\
                    Resolution: 3456 x 2234 Retina\n";
        assert_eq!(parse_resolution(text), Some((3456, 2234)));
    }

    #[test]
    fn reads_a_plain_resolution_line() {
        assert_eq!(
            parse_resolution("Resolution: 1920 x 1080\n"),
            Some((1920, 1080))
        );
    }

    #[test]
    fn takes_the_first_display_when_several_are_attached() {
        let text = "Resolution: 3456 x 2234 Retina\nResolution: 1920 x 1080\n";
        assert_eq!(parse_resolution(text), Some((3456, 2234)));
    }

    #[test]
    fn output_without_a_resolution_yields_nothing() {
        assert_eq!(parse_resolution("Chipset Model: Apple M4 Pro\n"), None);
        assert_eq!(parse_resolution(""), None);
    }

    #[test]
    fn a_malformed_resolution_is_not_half_parsed() {
        assert_eq!(parse_resolution("Resolution: wide x tall\n"), None);
        assert_eq!(parse_resolution("Resolution: 1920\n"), None);
    }

    #[test]
    fn the_timeout_error_names_the_likely_cause() {
        let msg = WallpaperError::TimedOut.to_string();
        assert!(msg.contains("display locked"), "{msg}");
    }

    #[test]
    fn a_single_desktop_is_read_as_itself() {
        assert_eq!(first_desktop("/a/b.png"), "/a/b.png");
    }

    #[test]
    fn several_desktops_report_the_first() {
        assert_eq!(first_desktop("/a/b.png, /c/d.png"), "/a/b.png");
    }

    #[test]
    fn paths_that_would_break_out_of_the_applescript_literal_are_refused() {
        let err = set(Path::new("/tmp/ev\"il.png")).unwrap_err();
        assert!(matches!(err, WallpaperError::Path));
    }
}
