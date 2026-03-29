use std::fs;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::cli::FixArgs;
use crate::cli::SetupArgs;
use crate::context::AppContext;
use crate::output::{Section, color_enabled, colorize, print_report};
use crate::rust_target;
use crate::sysroot;
use crate::tool_env::ensure_rust_target_installed;

pub fn run_fix(ctx: &AppContext, args: &FixArgs) -> Result<()> {
    if which::which("rustup").is_err() {
        bail!("rustup is not installed or not in PATH");
    }

    let use_color = color_enabled();
    let mut sections = Vec::new();

    // -- Prerequisites -------------------------------------------------------

    let mut prereq = Section::new("Prerequisites");
    prereq.ok("rustup");
    if which::which("rpmbuild").is_ok() {
        prereq.ok("rpmbuild");
    } else {
        let mut msg =
            "rpmbuild not found (install rpm-build) — only needed for cargo tizen rpm".to_string();
        if let Some(hint) = rpmbuild_install_hint_from_os_release() {
            msg.push_str(&format!(
                "\n  install with: {}",
                colorize(use_color, "1;36", &hint)
            ));
        }
        prereq.warn(msg);
    }
    sections.push(prereq);

    // -- Per-architecture fix ------------------------------------------------

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    for arch in arches {
        let mut sec = Section::new(format!("Architecture: {arch}"));

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
            Ok(true) => sec.ok(format!("rust target installed: {rust_target}")),
            Ok(false) => {
                ctx.info(format!("installing rust target: {rust_target}"));
                let status = Command::new("rustup")
                    .arg("target")
                    .arg("add")
                    .arg(&rust_target)
                    .status()
                    .with_context(|| format!("failed to run rustup target add {}", rust_target));

                match status {
                    Ok(s) if s.success() => {
                        sec.ok(format!("installed rust target: {rust_target}"));
                    }
                    Ok(s) => sec.error(format!(
                        "rustup target add {rust_target} failed with status: {s}"
                    )),
                    Err(err) => sec.error(format!("{err}")),
                }
            }
            Err(err) => sec.error(format!("failed to query rust targets: {err}")),
        }

        // Sysroot
        if sysroot::resolve_for_build(ctx, arch).is_ok() {
            sec.ok(format!("sysroot ready for {arch}"));
        } else {
            ctx.info(format!("preparing sysroot for {arch}"));
            let setup = SetupArgs {
                arch: Some(arch),
                profile: None,
                platform_version: None,
                provider: None,
                sdk_root: None,
                force: false,
            };
            match sysroot::run_setup(ctx, &setup) {
                Ok(()) => sec.ok(format!("sysroot prepared for {arch}")),
                Err(err) => sec.error_multiline(&format!("sysroot setup failed for {arch}: {err}")),
            }
        }

        sections.push(sec);
    }

    // -- Render output -------------------------------------------------------

    let error_count = print_report(&sections, use_color, false, None);

    if error_count > 0 {
        let total = sections.len();
        bail!("fix encountered issues in {error_count} of {total} categories")
    }
    Ok(())
}

fn rpmbuild_install_hint_from_os_release() -> Option<String> {
    let raw = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .ok()?;
    let parsed = parse_os_release(&raw);
    rpmbuild_install_hint(parsed.get("id"), parsed.get("id_like"))
}

fn rpmbuild_install_hint(id: Option<&String>, id_like: Option<&String>) -> Option<String> {
    let id = id.map(|v| v.to_ascii_lowercase()).unwrap_or_default();
    let id_like = id_like.map(|v| v.to_ascii_lowercase()).unwrap_or_default();

    let is_debian = ["ubuntu", "debian", "linuxmint", "pop", "elementary"].contains(&id.as_str())
        || id_like.contains("debian");
    if is_debian {
        return Some("sudo apt update && sudo apt install -y rpm".to_string());
    }

    let is_fedora_rhel = [
        "fedora",
        "rhel",
        "centos",
        "rocky",
        "almalinux",
        "ol",
        "amzn",
    ]
    .contains(&id.as_str())
        || id_like.contains("fedora")
        || id_like.contains("rhel");
    if is_fedora_rhel {
        return Some("sudo dnf install -y rpm-build".to_string());
    }

    let is_suse = id.contains("suse") || id_like.contains("suse");
    if is_suse {
        return Some("sudo zypper install -y rpm-build".to_string());
    }

    let is_arch = id == "arch" || id_like.contains("arch");
    if is_arch {
        return Some("sudo pacman -S --needed rpm-tools".to_string());
    }

    None
}

fn parse_os_release(raw: &str) -> std::collections::HashMap<String, String> {
    let mut values = std::collections::HashMap::new();
    for line in raw.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let cleaned = value.trim_matches('"').trim_matches('\'').to_string();
            values.insert(key.to_ascii_lowercase(), cleaned);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::{parse_os_release, rpmbuild_install_hint};

    #[test]
    fn parses_os_release_values() {
        let parsed = parse_os_release("ID=ubuntu\nID_LIKE=\"debian\"\n");
        assert_eq!(parsed.get("id").map(String::as_str), Some("ubuntu"));
        assert_eq!(parsed.get("id_like").map(String::as_str), Some("debian"));
    }

    #[test]
    fn picks_debian_hint() {
        let hint = rpmbuild_install_hint(Some(&"ubuntu".to_string()), Some(&"debian".to_string()))
            .expect("debian hint should be detected");
        assert!(hint.contains("apt install"));
    }

    #[test]
    fn picks_fedora_hint() {
        let hint = rpmbuild_install_hint(Some(&"fedora".to_string()), None)
            .expect("fedora hint should be detected");
        assert!(hint.contains("dnf install"));
    }
}
