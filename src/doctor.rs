use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::cli::DoctorArgs;
use crate::context::AppContext;
use crate::rust_target;
use crate::sdk::TizenSdk;
use crate::sysroot;
use crate::sysroot::provider::SetupRequest;
use crate::sysroot::rootstrap::{self, MISSING_SDK_GUIDANCE};
use crate::tool_env::{
    ensure_rust_target_installed, find_tool_in_sdk, resolve_toolchain, verify_c_compiler_sanity,
};

// ---------------------------------------------------------------------------
// Section model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    Ok,
    Warn,
    Error,
}

struct CheckItem {
    severity: Severity,
    message: String,
    detail: Vec<String>,
}

struct Section {
    title: String,
    items: Vec<CheckItem>,
}

impl Section {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    fn ok(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Ok,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Warn,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    fn error_multiline(&mut self, full: &str) {
        let mut lines = full.lines();
        let first = lines.next().unwrap_or(full).to_string();
        let detail: Vec<String> = lines.map(String::from).collect();
        self.items.push(CheckItem {
            severity: Severity::Error,
            message: first,
            detail,
        });
    }

    fn error(&mut self, message: impl Into<String>) {
        self.items.push(CheckItem {
            severity: Severity::Error,
            message: message.into(),
            detail: Vec::new(),
        });
    }

    fn severity(&self) -> Severity {
        self.items
            .iter()
            .map(|i| i.severity)
            .max()
            .unwrap_or(Severity::Ok)
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

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

fn render_sections(sections: &[Section], use_color: bool, verbose: bool) -> String {
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

// ---------------------------------------------------------------------------
// Doctor checks
// ---------------------------------------------------------------------------

pub fn run_doctor(ctx: &AppContext, args: &DoctorArgs) -> Result<()> {
    let use_color = color_output_enabled();
    let verbose = ctx.verbose;
    let mut sections = Vec::new();

    // -- Host tools ----------------------------------------------------------

    let mut host = Section::new("Host tools");
    let mut found_tools = Vec::new();
    let mut missing_tools = Vec::new();
    for tool in ["cargo", "rustc", "rustup"] {
        if which::which(tool).is_ok() {
            found_tools.push(tool);
        } else {
            missing_tools.push(tool);
        }
    }
    if missing_tools.is_empty() {
        host.ok(found_tools.join(", "));
    } else {
        for tool in &missing_tools {
            host.error(format!("missing: {tool}"));
        }
        if !found_tools.is_empty() {
            host.ok(found_tools.join(", "));
        }
    }
    if which::which("rpmbuild").is_ok() {
        host.ok("rpmbuild");
    } else {
        host.warn("rpmbuild not found (install rpm-build) — only needed for cargo tizen rpm");
    }
    sections.push(host);

    // -- Tizen SDK -----------------------------------------------------------

    let mut sdk_section = Section::new("Tizen SDK");
    let sdk = TizenSdk::locate(ctx.config.sdk_root().as_deref());
    match &sdk {
        Some(sdk) => {
            sdk_section.ok(format!("{} ({})", sdk.root().display(), sdk.flavor()));
            let tizen_cli = sdk.tizen_cli();
            if tizen_cli.is_file() {
                sdk_section.ok(format!("tizen CLI: {}", tizen_cli.display()));
            } else {
                sdk_section.warn(format!(
                    "tizen CLI not found at expected path: {}",
                    tizen_cli.display()
                ));
            }
        }
        None => sdk_section.error_multiline(MISSING_SDK_GUIDANCE),
    }
    sections.push(sdk_section);

    // -- Rootstrap coverage --------------------------------------------------

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
        let cov_section = build_rootstrap_coverage_section(ctx, &arches);
        sections.push(cov_section);
    }

    // -- Per-architecture checks ---------------------------------------------

    for arch in &arches {
        let arch = *arch;
        let mut sec = Section::new(format!("Architecture: {arch}"));
        let toolchain = resolve_toolchain(ctx, arch);
        let linker = toolchain.linker;

        // Linker
        if binary_exists(&linker) {
            sec.ok(format!("linker: {linker}"));
        } else {
            let mut message = format!("linker not found: {linker}");
            if let Some(sdk) = &sdk {
                let default_linker = ctx.config.linker_for(arch);
                if let Some(found) = find_tool_in_sdk(sdk, &default_linker) {
                    message.push_str(&format!(
                        " (candidate in SDK: {}; set [arch.{}].linker)",
                        found.display(),
                        arch
                    ));
                }
            }
            sec.error(message);
        }

        // Rust target
        let rust_target = match rust_target::resolve_for_arch(ctx, arch) {
            Ok(target) => target,
            Err(err) => {
                sec.error(format!("failed to resolve rust target: {err}"));
                sections.push(sec);
                continue;
            }
        };
        match ensure_rust_target_installed(&rust_target) {
            Ok(true) => sec.ok(format!("rust target: {rust_target}")),
            Ok(false) => {
                sec.error(format!(
                    "rust target not installed: {rust_target} (try: rustup target add {rust_target})"
                ));
            }
            Err(err) => sec.error(format!("failed to query rust targets: {err}")),
        }

        // Rootstrap resolution
        if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
            let sdk_root_override = ctx.config.sdk_root();
            match sysroot::resolve_profile_platform_for_arch(ctx, arch) {
                Ok((profile, platform_version)) => {
                    let req = SetupRequest {
                        arch,
                        profile,
                        platform_version,
                        sdk_root_override,
                    };
                    match rootstrap::resolve_rootstrap(&req) {
                        Ok(resolved) => {
                            let mut msg = format!(
                                "rootstrap: {} ({})",
                                resolved.id,
                                resolved.root_path.display()
                            );
                            if resolved.used_fallback {
                                msg.push_str(" [fallback]");
                            }
                            sec.ok(msg);
                        }
                        Err(err) => sec.error_multiline(&format!("rootstrap: {err}")),
                    }
                }
                Err(err) => {
                    sec.error(format!("profile/platform resolution failed: {err}"));
                }
            }
        }

        // Sysroot cache + compiler sanity
        match sysroot::resolve_for_build(ctx, arch) {
            Ok(resolved) => {
                sec.ok(format!("sysroot cache: {}", resolved.sysroot_dir.display()));
                match verify_c_compiler_sanity(&toolchain.cc, Some(&resolved.sysroot_dir)) {
                    Ok(()) => sec.ok(format!("C compiler: {}", toolchain.cc)),
                    Err(err) => sec.error(format!("C compiler sanity failed: {err}")),
                }
            }
            Err(err) => sec.error_multiline(&format!("sysroot: {err}")),
        }

        sections.push(sec);
    }

    // -- Render output -------------------------------------------------------

    if !verbose {
        println!(
            "{}",
            colorize(
                use_color,
                "2",
                "Doctor summary (to see all details, run cargo tizen doctor -v):"
            )
        );
        println!();
    }

    let rendered = render_sections(&sections, use_color, verbose);
    print!("{rendered}");

    let error_count = sections
        .iter()
        .filter(|s| s.severity() == Severity::Error)
        .count();
    let total = sections.len();

    if error_count == 0 {
        println!(
            "\n{}",
            colorize(
                use_color,
                "1;32",
                &format!("✓ Doctor found no issues ({total} categories checked).")
            )
        );
        Ok(())
    } else {
        println!(
            "\n{}",
            colorize(
                use_color,
                "1;31",
                &format!(
                    "✗ Doctor found issues in {} of {} categories.",
                    error_count, total
                )
            )
        );
        bail!("doctor found issues in {error_count} of {total} categories")
    }
}

fn build_rootstrap_coverage_section(ctx: &AppContext, arches: &[Arch]) -> Section {
    let mut sec = Section::new("Rootstrap coverage");
    let sdk_root_override = ctx.config.sdk_root();
    let mut grouped: BTreeMap<(String, String), BTreeSet<Arch>> = BTreeMap::new();
    let mut any_warning = false;

    for arch in arches.iter().copied() {
        let options =
            match rootstrap::installed_rootstrap_options(sdk_root_override.as_deref(), arch) {
                Ok(options) => options,
                Err(err) => {
                    sec.error(format!("failed to discover rootstraps for {arch}: {err}"));
                    continue;
                }
            };

        if options.is_empty() {
            sec.warn(format!("no rootstrap targets found for {arch}"));
            any_warning = true;
            continue;
        }

        for option in options {
            grouped
                .entry((option.platform_version.clone(), option.profile.clone()))
                .or_default()
                .insert(arch);
        }
    }

    if grouped.is_empty() && !any_warning && sec.items.is_empty() {
        sec.warn("no rootstrap targets discovered");
        return sec;
    }

    let mut keys = grouped.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|a, b| {
        version_sort_key(&b.0)
            .cmp(&version_sort_key(&a.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    for key in keys {
        if let Some(arch_entries) = grouped.get(&key) {
            let arches_str = arches
                .iter()
                .copied()
                .filter(|arch| arch_entries.contains(arch))
                .map(|arch| arch.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            sec.ok(format!(
                "--platform-version {} --profile {} ({})",
                key.0, key.1, arches_str
            ));
        }
    }

    sec
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn version_sort_key(version: &str) -> (u64, u64) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    (major, minor)
}

fn color_output_enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn colorize(enabled: bool, ansi_code: &str, value: &str) -> String {
    if enabled {
        return format!("\x1b[{}m{}\x1b[0m", ansi_code, value);
    }
    value.to_string()
}

fn binary_exists(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') {
        return Path::new(value).is_file();
    }
    which::which(value).is_ok()
}
