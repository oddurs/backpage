//! Composition of dashboard frames.
//!
//! A [`Frame`] is a fixed character grid that [`Panel`]s are stacked into. The
//! result is plain text; turning that text into a desktop picture is a separate
//! concern and is not implemented yet.

/// Smallest frame width that can still show a border and one content column.
pub const MIN_WIDTH: usize = 8;

/// Smallest frame height that can hold a single panel's borders.
pub const MIN_HEIGHT: usize = 2;

/// Characters consumed by a panel's left and right border plus their padding.
const BORDER_COST: usize = 4;

/// A titled block of rows drawn inside a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    title: String,
    rows: Vec<String>,
}

impl Panel {
    /// Creates a panel with the given title and rows.
    #[must_use]
    pub fn new(title: impl Into<String>, rows: Vec<String>) -> Self {
        Self {
            title: title.into(),
            rows,
        }
    }

    /// Number of lines this panel occupies once rendered, borders included.
    #[must_use]
    pub fn height(&self) -> usize {
        self.rows.len() + 2
    }

    /// Width needed to show the title and every row without truncation.
    #[must_use]
    pub fn natural_width(&self) -> usize {
        let widest = self
            .rows
            .iter()
            .map(|r| r.chars().count())
            .chain([self.title.chars().count()])
            .max();
        widest.unwrap_or(0) + BORDER_COST
    }

    /// Renders the panel as `width`-wide lines.
    ///
    /// Titles and rows longer than the available space are truncated with an
    /// ellipsis. `width` is clamped to [`MIN_WIDTH`].
    #[must_use]
    pub fn render(&self, width: usize) -> Vec<String> {
        let width = width.max(MIN_WIDTH);
        let inner = width - BORDER_COST;

        let title = truncate(&self.title, inner);
        let used = title.chars().count() + 2;
        let mut top = String::from("\u{250c}");
        top.push(' ');
        top.push_str(&title);
        top.push(' ');
        top.extend(std::iter::repeat_n('\u{2500}', width - 2 - used));
        top.push('\u{2510}');

        let mut lines = vec![top];
        for row in &self.rows {
            let text = truncate(row, inner);
            let pad = inner - text.chars().count();
            let mut line = String::from("\u{2502} ");
            line.push_str(&text);
            line.extend(std::iter::repeat_n(' ', pad));
            line.push_str(" \u{2502}");
            lines.push(line);
        }

        let mut bottom = String::from("\u{2514}");
        bottom.extend(std::iter::repeat_n('\u{2500}', width - 2));
        bottom.push('\u{2518}');
        lines.push(bottom);

        lines
    }
}

/// A fixed-size character grid that panels are laid out into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    width: usize,
    height: usize,
}

impl Frame {
    /// Creates a frame, clamping to [`MIN_WIDTH`] and [`MIN_HEIGHT`].
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width: width.max(MIN_WIDTH),
            height: height.max(MIN_HEIGHT),
        }
    }

    /// Stacks `panels` top to bottom and pads the result to the full height.
    ///
    /// Panels that do not fit are dropped rather than clipped mid-border, so
    /// every line of the output is a complete row of the grid.
    #[must_use]
    pub fn render(&self, panels: &[Panel]) -> String {
        let mut lines: Vec<String> = Vec::with_capacity(self.height);
        for panel in panels {
            if lines.len() + panel.height() > self.height {
                break;
            }
            lines.extend(panel.render(self.width));
        }
        let blank = " ".repeat(self.width);
        while lines.len() < self.height {
            lines.push(blank.clone());
        }
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }
}

/// Shortens `s` to at most `max` characters, marking the cut with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(lines: &[String]) -> Vec<usize> {
        lines.iter().map(|l| l.chars().count()).collect()
    }

    #[test]
    fn panel_puts_its_title_in_the_top_border() {
        let lines = Panel::new("cpu", vec![]).render(20);
        assert!(
            lines[0].starts_with("\u{250c} cpu \u{2500}"),
            "got {:?}",
            lines[0]
        );
    }

    #[test]
    fn every_rendered_line_is_exactly_the_requested_width() {
        let panel = Panel::new("disk", vec!["a".into(), "bb".into()]);
        assert_eq!(widths(&panel.render(24)), vec![24; 4]);
    }

    #[test]
    fn long_rows_are_truncated_with_an_ellipsis() {
        let panel = Panel::new("net", vec!["0123456789".into()]);
        let lines = panel.render(MIN_WIDTH);
        assert_eq!(lines[1], "\u{2502} 012\u{2026} \u{2502}");
    }

    #[test]
    fn long_titles_are_truncated_too() {
        let lines = Panel::new("a-very-long-title", vec![]).render(MIN_WIDTH);
        assert_eq!(widths(&lines), vec![MIN_WIDTH; 2]);
    }

    #[test]
    fn natural_width_covers_the_widest_row_plus_the_border() {
        let panel = Panel::new("t", vec!["abc".into(), "abcdefgh".into()]);
        assert_eq!(panel.natural_width(), 8 + BORDER_COST);
    }

    #[test]
    fn natural_width_accounts_for_a_title_wider_than_every_row() {
        let panel = Panel::new("a-long-title", vec!["x".into()]);
        assert_eq!(panel.natural_width(), "a-long-title".len() + BORDER_COST);
    }

    #[test]
    fn rendering_at_the_natural_width_truncates_nothing() {
        let panel = Panel::new("t", vec!["abcdefgh".into()]);
        let lines = panel.render(panel.natural_width());
        assert!(lines[1].contains("abcdefgh"), "got {:?}", lines[1]);
        assert!(!lines[1].contains('\u{2026}'));
    }

    #[test]
    fn panel_height_counts_both_borders() {
        assert_eq!(Panel::new("t", vec!["x".into(), "y".into()]).height(), 4);
    }

    #[test]
    fn frame_pads_unused_space_to_the_full_height() {
        let frame = Frame::new(10, 6);
        let out = frame.render(&[Panel::new("t", vec!["x".into()])]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[3], "          ");
    }

    #[test]
    fn frame_drops_panels_that_do_not_fit_rather_than_clipping_them() {
        let frame = Frame::new(10, 3);
        let out = frame.render(&[Panel::new("a", vec![]), Panel::new("b", vec![])]);
        assert_eq!(out.matches('\u{2514}').count(), 1);
    }

    #[test]
    fn frame_clamps_dimensions_below_the_minimum() {
        let out = Frame::new(0, 0).render(&[]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), MIN_HEIGHT);
        assert_eq!(
            widths(&lines.iter().map(|l| (*l).to_string()).collect::<Vec<_>>()),
            vec![MIN_WIDTH; MIN_HEIGHT]
        );
    }

    #[test]
    fn output_ends_with_a_trailing_newline() {
        assert!(Frame::new(10, 2).render(&[]).ends_with('\n'));
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "ab\u{2026}");
        assert_eq!(truncate("abcd", 0), "");
    }
}
