//! Reading one sample from `poptop --once`.
//!
//! poptop is a system monitor with a non-interactive mode intended for scripts.
//! backpage shells out to it rather than collecting anything itself, so there is
//! exactly one implementation of "what is this machine doing".

use std::fmt;
use std::process::Command;

/// A parsed `poptop --once` sample.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sample {
    /// Headline statistics, one `key value` line each.
    pub stats: Vec<String>,
    /// The process table, header row first.
    pub processes: Vec<String>,
}

/// Why a sample could not be read.
#[derive(Debug)]
pub enum SourceError {
    /// The source program could not be run at all.
    Spawn {
        /// The program that was attempted.
        program: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The source program ran but exited non-zero.
    Failed {
        /// The program that was run.
        program: String,
        /// Its exit status, rendered.
        status: String,
        /// Whatever it wrote to stderr.
        stderr: String,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { program, source } => {
                write!(f, "could not run {program}: {source}")
            }
            Self::Failed {
                program,
                status,
                stderr,
            } => {
                write!(f, "{program} exited {status}: {}", stderr.trim())
            }
        }
    }
}

impl std::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::Failed { .. } => None,
        }
    }
}

/// Runs `<program> --once` and parses the result.
///
/// # Errors
///
/// Returns [`SourceError`] if the program cannot be spawned or exits non-zero.
pub fn read(program: &str) -> Result<Sample, SourceError> {
    let output = Command::new(program)
        .arg("--once")
        .output()
        .map_err(|source| SourceError::Spawn {
            program: program.to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(SourceError::Failed {
            program: program.to_string(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

/// Splits a sample into headline statistics and the process table.
///
/// Statistics whose value begins with an em dash are dropped: that is how
/// poptop reports a figure the platform does not publish, and a wallpaper is
/// the wrong place for two dozen lines saying "not published here".
#[must_use]
pub fn parse(raw: &str) -> Sample {
    let mut sample = Sample::default();
    let mut in_table = false;

    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }

        if !in_table && is_table_header(trimmed) {
            in_table = true;
        }

        if in_table {
            // Kept verbatim: poptop right-aligns the numeric columns, so
            // trimming each row individually would destroy the alignment.
            sample.processes.push(trimmed.to_string());
        } else if is_available(trimmed) {
            sample.stats.push(trimmed.to_string());
        }
    }

    dedent(&mut sample.processes);
    sample
}

/// Removes the indentation common to every line, preserving relative alignment.
fn dedent(lines: &mut [String]) {
    let Some(common) = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
    else {
        return;
    };
    if common == 0 {
        return;
    }
    for line in lines {
        *line = line.get(common..).unwrap_or("").to_string();
    }
}

/// Whether a line is the process table's header row.
fn is_table_header(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("PID") && t.contains("COMMAND")
}

/// Whether a statistic carries a real figure rather than an em dash.
fn is_available(line: &str) -> bool {
    match line.split_once(char::is_whitespace) {
        Some((_, rest)) => !rest.trim_start().starts_with('\u{2014}'),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "\
cpu     61.3%  (14 cores)
mem     81.0%  19.4G / 24.0G used
steal   \u{2014}  not published here
switch  \u{2014} context, \u{2014} interrupts
procs   724
io     192/724 processes unreadable \u{2014} run as root to see them

    PID    CPU%        RSS  COMMAND
    726    24.0      44.0M  NotificationCenter
  96954    18.7     441.8M  OrbStack Helper
";

    #[test]
    fn keeps_statistics_that_carry_a_figure() {
        let s = parse(RAW);
        assert!(s.stats.iter().any(|l| l.starts_with("cpu")));
        assert!(s.stats.iter().any(|l| l.starts_with("mem")));
        assert!(s.stats.iter().any(|l| l.starts_with("procs")));
    }

    #[test]
    fn drops_statistics_the_platform_does_not_publish() {
        let s = parse(RAW);
        assert!(!s.stats.iter().any(|l| l.starts_with("steal")));
        assert!(!s.stats.iter().any(|l| l.starts_with("switch")));
    }

    #[test]
    fn an_em_dash_inside_a_real_value_does_not_drop_the_line() {
        // The io line reports a real count and then explains a caveat.
        let s = parse(RAW);
        assert!(
            s.stats.iter().any(|l| l.starts_with("io")),
            "stats: {:?}",
            s.stats
        );
    }

    #[test]
    fn the_table_keeps_its_columns_aligned() {
        let s = parse(RAW);
        // Every row's CPU% column should end at the same offset.
        let cpu_end = |l: &str| l.find("24.0").or_else(|| l.find("18.7")).map(|i| i + 4);
        let a = cpu_end(&s.processes[1]).expect("row 1 has a cpu figure");
        let b = cpu_end(&s.processes[2]).expect("row 2 has a cpu figure");
        assert_eq!(a, b, "columns drifted: {:?}", s.processes);
    }

    #[test]
    fn the_common_indent_is_removed_so_nothing_is_wasted_on_a_margin() {
        let s = parse(RAW);
        assert!(
            s.processes.iter().all(|l| !l.starts_with("    ")),
            "{:?}",
            s.processes
        );
    }

    #[test]
    fn the_process_table_starts_at_its_header() {
        let s = parse(RAW);
        assert!(s.processes[0].trim_start().starts_with("PID"));
        assert_eq!(s.processes.len(), 3);
    }

    #[test]
    fn table_rows_are_never_treated_as_statistics() {
        let s = parse(RAW);
        assert!(!s.stats.iter().any(|l| l.contains("NotificationCenter")));
    }

    #[test]
    fn an_empty_sample_parses_to_nothing() {
        assert_eq!(parse(""), Sample::default());
    }
}
