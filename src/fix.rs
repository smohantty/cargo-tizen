use std::fs;
use std::io::IsTerminal;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::cli::FixArgs;
use crate::cli::SetupArgs;
use crate::context::AppContext;
use crate::rust_target;
use crate::sysroot;
use crate::tool_env::ensure_rust_target_installed;

pub fn run_fix(ctx: &AppContext, args: &FixArgs) -> Result<()> {
    if which::which("rustup").is_err() {
        bail!("rustup is not installed or not in PATH");
    }

    warn_missing_rpmbuild();

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    let mut missing_targets = Vec::new();
    let mut failures = Vec::new();
    for arch in arches {
        let rust_target = match rust_target::resolve_for_arch(ctx, arch) {
            Ok(target) => target,
            Err(err) => {
                failures.push(format!(
                    "failed to resolve rust target for {}: {}",
                    arch, err
                ));
                continue;
            }
        };
        if ensure_rust_target_installed(&rust_target)? {
            ctx.info(format!(
                "[ok] rust target already installed for {}: {}",
                arch, rust_target
            ));
        } else {
            missing_targets.push((arch, rust_target));
        }

        if let Err(err) = ensure_sysroot_ready(ctx, arch) {
            failures.push(format!("failed to prepare sysroot for {}: {}", arch, err));
        }
    }

    for (arch, rust_target) in missing_targets {
        ctx.info(format!(
            "installing missing rust target for {}: {}",
            arch, rust_target
        ));
        let status = Command::new("rustup")
            .arg("target")
            .arg("add")
            .arg(&rust_target)
            .status()
            .with_context(|| format!("failed to run rustup target add {}", rust_target));

        match status {
            Ok(status) if status.success() => {
                ctx.info(format!("[ok] installed rust target {}", rust_target));
            }
            Ok(status) => failures.push(format!(
                "rustup target add {} failed with status: {}",
                rust_target, status
            )),
            Err(err) => failures.push(err.to_string()),
        }
    }

    if failures.is_empty() {
        ctx.info("fix completed");
        return Ok(());
    }

    for failure in &failures {
        eprintln!("[error] {failure}");
    }
    bail!("fix found {} issue(s)", failures.len())
}

fn ensure_sysroot_ready(ctx: &AppContext, arch: Arch) -> Result<()> {
    if sysroot::resolve_for_build(ctx, arch).is_ok() {
        ctx.info(format!("[ok] sysroot already ready for {}", arch));
        return Ok(());
    }

    ctx.info(format!("preparing sysroot for {}", arch));
    let setup = SetupArgs {
        arch: Some(arch),
        profile: None,
        platform_version: None,
        provider: None,
        sdk_root: None,
        force: false,
    };
    sysroot::run_setup(ctx, &setup)
}

fn warn_missing_rpmbuild() {
    if which::which("rpmbuild").is_ok() {
        return;
    }

    let use_color = color_output_enabled();
    eprintln!("[warn] missing tool: rpmbuild [required only for `cargo tizen rpm`]");
    if let Some(hint) = rpmbuild_install_hint_from_os_release() {
        eprintln!(
            "[warn] install with: {}",
            colorize(use_color, "1;36", &hint)
        );
    } else {
        eprintln!(
            "[warn] install your distro package that provides {}",
            colorize(use_color, "1;36", "`rpmbuild` (commonly `rpm-build`)")
        );
    }
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

fn color_output_enabled() -> bool {
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn colorize(enabled: bool, ansi_code: &str, value: &str) -> String {
    if enabled {
        return format!("\x1b[{}m{}\x1b[0m", ansi_code, value);
    }
    value.to_string()
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
