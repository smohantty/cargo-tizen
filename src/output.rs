use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Ok,
    Warn,
    Error,
}

pub struct CheckItem {
    pub severity: Severity,
    pub message: String,
    pub detail: Vec<String>,
}

pub struct Section {
    pub title: String,
    pub items: Vec<CheckItem>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    pub fn ok(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Ok,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Warn,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    pub fn warn_multiline(&mut self, full: &str) {
        let mut lines = full.lines();
        let first = lines.next().unwrap_or(full).to_string();
        let detail: Vec<String> = lines.map(String::from).collect();
        self.items.push(CheckItem {
            severity: Severity::Warn,
            message: first,
            detail,
        });
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Error,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    pub fn error_multiline(&mut self, full: &str) {
        let mut lines = full.lines();
        let first = lines.next().unwrap_or(full).to_string();
        let detail: Vec<String> = lines.map(String::from).collect();
        self.items.push(CheckItem {
            severity: Severity::Error,
            message: first,
            detail,
        });
    }

    pub fn severity(&self) -> Severity {
        self.items
            .iter()
            .map(|i| i.severity)
            .max()
            .unwrap_or(Severity::Ok)
    }
}

fn severity_bracket(sev: Severity, color: bool) -> String {
    match sev {
        Severity::Ok => colorize(color, "1;32", "[✓]"),
        Severity::Warn => colorize(color, "1;33", "[!]"),
        Severity::Error => colorize(color, "1;31", "[✗]"),
    }
}

fn item_marker(sev: Severity, color: bool) -> String {
    match sev {
        Severity::Ok => colorize(color, "32", "•"),
        Severity::Warn => colorize(color, "33", "!"),
        Severity::Error => colorize(color, "31", "✗"),
    }
}

pub fn render_sections(sections: &[Section], use_color: bool, verbose: bool) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let bracket = severity_bracket(section.severity(), use_color);
        let title = colorize(use_color, "1", &section.title);
        out.push_str(&format!("{} {}\n", bracket, title));

        for item in &section.items {
            if item.severity == Severity::Ok && !verbose {
                continue;
            }
            let marker = item_marker(item.severity, use_color);
            out.push_str(&format!("    {} {}\n", marker, item.message));
            for line in &item.detail {
                out.push_str(&format!("      {}\n", line));
            }
        }
    }
    out
}

pub fn color_enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub fn colorize(enabled: bool, ansi_code: &str, value: &str) -> String {
    if enabled {
        format!("\x1b[{}m{}\x1b[0m", ansi_code, value)
    } else {
        value.to_string()
    }
}

/// Render a right-aligned 15-character status label in bold bright green,
/// matching the Cargo output convention used by `Compiling`, `Finished`, etc.
pub fn cargo_status(use_color: bool, status: &str) -> String {
    colorize(use_color, "1;92", &format!("{status:>15}"))
}

/// Print sections and a summary line. Returns the count of sections with errors.
pub fn print_report(
    sections: &[Section],
    use_color: bool,
    verbose: bool,
    summary_hint: Option<&str>,
) -> usize {
    if let Some(hint) = summary_hint {
        if !verbose {
            println!("{}", colorize(use_color, "2", hint));
            println!();
        }
    }

    let rendered = render_sections(sections, use_color, verbose);
    print!("{rendered}");

    let total = sections.len();
    let mut error_count = 0;
    let mut warn_count = 0;
    for s in sections {
        match s.severity() {
            Severity::Error => error_count += 1,
            Severity::Warn => warn_count += 1,
            Severity::Ok => {}
        }
    }

    if error_count > 0 {
        println!(
            "\n{}",
            colorize(
                use_color,
                "1;31",
                &format!("✗ Issues found in {} of {} categories.", error_count, total),
            )
        );
    } else if warn_count > 0 {
        println!(
            "\n{}",
            colorize(
                use_color,
                "1;33",
                &format!(
                    "✓ Passed with warnings in {} of {total} categories.",
                    warn_count
                ),
            )
        );
    } else {
        println!(
            "\n{}",
            colorize(
                use_color,
                "1;32",
                &format!("✓ No issues ({total} categories checked)."),
            )
        );
    }

    error_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_severity_defaults_to_ok_when_empty() {
        let section = Section::new("Empty");
        assert_eq!(section.severity(), Severity::Ok);
    }

    #[test]
    fn section_severity_is_max_of_items() {
        let mut section = Section::new("Mixed");
        section.ok("all good");
        section.warn("something off");
        assert_eq!(section.severity(), Severity::Warn);

        section.error("bad");
        assert_eq!(section.severity(), Severity::Error);
    }

    #[test]
    fn warn_multiline_splits_first_line_as_message() {
        let mut section = Section::new("Test");
        section.warn_multiline("first line\nsecond line\nthird line");
        assert_eq!(section.items.len(), 1);
        assert_eq!(section.items[0].message, "first line");
        assert_eq!(section.items[0].detail, vec!["second line", "third line"]);
    }

    #[test]
    fn error_multiline_splits_first_line_as_message() {
        let mut section = Section::new("Test");
        section.error_multiline("error here\ndetail 1\ndetail 2");
        assert_eq!(section.items[0].message, "error here");
        assert_eq!(section.items[0].detail, vec!["detail 1", "detail 2"]);
        assert_eq!(section.items[0].severity, Severity::Error);
    }

    #[test]
    fn colorize_disabled_returns_plain_text() {
        assert_eq!(colorize(false, "1;32", "hello"), "hello");
    }

    #[test]
    fn colorize_enabled_wraps_with_ansi() {
        let result = colorize(true, "1;32", "hello");
        assert!(result.starts_with("\x1b[1;32m"));
        assert!(result.ends_with("\x1b[0m"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn cargo_status_right_aligns_to_15_chars() {
        let result = cargo_status(false, "Building");
        assert_eq!(result.len(), 15);
        assert!(result.ends_with("Building"));
    }

    #[test]
    fn render_sections_hides_ok_items_in_non_verbose() {
        let mut section = Section::new("Test");
        section.ok("good item");
        section.warn("warning item");
        let rendered = render_sections(&[section], false, false);
        assert!(!rendered.contains("good item"));
        assert!(rendered.contains("warning item"));
    }

    #[test]
    fn render_sections_shows_ok_items_in_verbose() {
        let mut section = Section::new("Test");
        section.ok("good item");
        section.warn("warning item");
        let rendered = render_sections(&[section], false, true);
        assert!(rendered.contains("good item"));
        assert!(rendered.contains("warning item"));
    }

    #[test]
    fn render_sections_separates_multiple_sections() {
        let s1 = Section::new("First");
        let s2 = Section::new("Second");
        let rendered = render_sections(&[s1, s2], false, false);
        assert!(rendered.contains("First"));
        assert!(rendered.contains("Second"));
        // Second section should be preceded by a blank line
        assert!(rendered.contains("\n\n"));
    }
}
