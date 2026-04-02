use std::io::IsTerminal;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
