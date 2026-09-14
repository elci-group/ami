use form3::ansi::Color;
use form3::compat::Colorize;
use form3::table::{Attribute, Cell, Table, TableStyle};
use form3::term::ColorSupport;
use std::env;
use std::io::{self, IsTerminal};

/// Consistent terminal presentation for Ami's human-readable CLI output.
///
/// Colour is enabled only for an interactive terminal unless explicitly
/// requested with AMI_COLOR=1. It is disabled by NO_COLOR and TERM=dumb.
pub struct Output {
    color: bool,
}

const MAX_CELL_WIDTH: usize = 64;

impl Output {
    pub fn new() -> Self {
        let forced = env::var("AMI_COLOR").ok().as_deref() == Some("1");
        let disabled = !forced
            && (env::var_os("NO_COLOR").is_some()
                || env::var("TERM").ok().as_deref() == Some("dumb"));
        Self {
            color: !disabled && (forced || io::stdout().is_terminal()),
        }
    }

    pub fn heading(&self, icon: &str, text: impl AsRef<str>) -> String {
        format!("{} {}", icon, self.paint_fg(text.as_ref(), Color::Cyan))
    }

    pub fn success(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint_fg("✓", Color::Green), text.as_ref())
    }

    pub fn info(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint_fg("•", Color::Blue), text.as_ref())
    }

    pub fn warning(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint_fg("!", Color::Yellow), text.as_ref())
    }

    pub fn badge(&self, label: impl AsRef<str>, enabled: bool) -> String {
        let value = format!("[{}]", label.as_ref());
        if enabled {
            self.paint_fg(&value, Color::Green)
        } else {
            self.paint_dim(&value)
        }
    }

    pub fn table(&self, headers: &[&str], rows: &[Vec<String>]) -> String {
        if headers.is_empty() {
            return String::new();
        }

        let mut table = Table::new();
        table.set_style(TableStyle::Square);
        table.set_header(
            headers
                .iter()
                .map(|header| {
                    let cell = Cell::new(Self::normalize_cell(header));
                    if self.color {
                        cell.fg(Color::Cyan).add_attribute(Attribute::Bold)
                    } else {
                        cell
                    }
                })
                .collect::<Vec<_>>(),
        );
        for row in rows {
            let mut cells = row
                .iter()
                .map(|cell| Cell::new(Self::normalize_cell(cell)))
                .collect::<Vec<_>>();
            cells.resize_with(headers.len(), || Cell::new(""));
            table.add_row(cells);
        }

        let mut rendered = table.to_string();
        if rendered.ends_with('\n') {
            rendered.pop();
        }
        rendered
    }

    fn color_support(&self) -> ColorSupport {
        if self.color {
            ColorSupport::Standard16
        } else {
            ColorSupport::NoColor
        }
    }

    fn paint_fg(&self, text: &str, color: Color) -> String {
        text.color(color)
            .bold()
            .with_color_support(self.color_support())
            .to_string()
    }

    fn paint_dim(&self, text: &str) -> String {
        text.dimmed()
            .with_color_support(self.color_support())
            .to_string()
    }

    fn normalize_cell(value: &str) -> String {
        let clean = value
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>();
        if clean.chars().count() <= MAX_CELL_WIDTH {
            clean
        } else {
            let shortened = clean.chars().take(MAX_CELL_WIDTH - 1).collect::<String>();
            format!("{}…", shortened)
        }
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Output;

    #[test]
    fn table_is_aligned_and_plain_by_default() {
        let output = Output { color: false };
        let table = output.table(
            &["Name", "Score"],
            &[vec!["ami".to_string(), "95%".to_string()]],
        );
        assert!(table.contains("│ Name │ Score │"));
        assert!(!table.contains("\x1b["));
    }

    #[test]
    fn table_bounds_long_and_control_character_cells() {
        let output = Output { color: false };
        let table = output.table(&["Phrase"], &[vec![format!("{}\x01", "x".repeat(100))]]);
        assert!(!table.contains('\x01'));
        assert!(table.lines().all(|line| line.chars().count() <= 68));
    }
}
