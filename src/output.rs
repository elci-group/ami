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
        format!("{} {}", icon, self.paint("1;36", text.as_ref()))
    }

    pub fn success(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint("1;32", "✓"), text.as_ref())
    }

    pub fn info(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint("1;34", "•"), text.as_ref())
    }

    pub fn warning(&self, text: impl AsRef<str>) -> String {
        format!("{} {}", self.paint("1;33", "!"), text.as_ref())
    }

    pub fn badge(&self, label: impl AsRef<str>, enabled: bool) -> String {
        let (code, value) = if enabled {
            ("1;32", format!("[{}]", label.as_ref()))
        } else {
            ("2", format!("[{}]", label.as_ref()))
        };
        self.paint(code, &value)
    }

    pub fn table(&self, headers: &[&str], rows: &[Vec<String>]) -> String {
        if headers.is_empty() {
            return String::new();
        }

        let normalized_rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| Self::normalize_cell(cell))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let widths: Vec<usize> = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                normalized_rows
                    .iter()
                    .map(|row| row.get(index).map_or(0, |cell| cell.chars().count()))
                    .max()
                    .unwrap_or(0)
                    .max(Self::normalize_cell(header).chars().count())
            })
            .collect();

        let border = |left: char, middle: char, right: char| {
            format!(
                "{}{}{}",
                left,
                widths
                    .iter()
                    .map(|width| "─".repeat(width + 2))
                    .collect::<Vec<_>>()
                    .join(&middle.to_string()),
                right
            )
        };
        let row = |cells: Vec<String>, header: bool| {
            let rendered = cells
                .into_iter()
                .enumerate()
                .map(|(index, cell)| {
                    let padded = format!(" {:width$} ", cell, width = widths[index]);
                    if header {
                        self.paint("1;36", &padded)
                    } else {
                        padded
                    }
                })
                .collect::<Vec<_>>();
            format!("│{}│", rendered.join("│"))
        };

        let mut lines = vec![
            border('┌', '┬', '┐'),
            row(headers.iter().map(|h| h.to_string()).collect(), true),
        ];
        lines.push(border('├', '┼', '┤'));
        for values in &normalized_rows {
            let mut cells = values.clone();
            cells.resize(headers.len(), String::new());
            lines.push(row(cells, false));
        }
        lines.push(border('└', '┴', '┘'));
        lines.join("\n")
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{}m{}\x1b[0m", code, text)
        } else {
            text.to_string()
        }
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
