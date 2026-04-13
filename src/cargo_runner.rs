use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::arch::Arch;
use crate::arch_detect;
use crate::cli::BuildArgs;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled, colorize};
use crate::rust_target;
use crate::sysroot;
use crate::tool_env::{
    ToolEnv, ensure_rust_target_installed, resolve_toolchain, verify_c_compiler_sanity,
};

pub fn run_build(ctx: &AppContext, args: &BuildArgs) -> Result<()> {
    let arch = arch_detect::resolve_arch(ctx, args.arch, "build")?;
    let resolved = sysroot::ensure_for_build(ctx, arch)?;
    let rust_target =
        rust_target::resolve_with_sysroot_hint(ctx, arch, Some(&resolved.sysroot_dir))?;
    let toolchain = resolve_toolchain(ctx, arch);
    let target_dir = resolve_target_dir(&ctx.workspace_root, arch, args.target_dir.as_deref());
    let build_profile = if args.release { "release" } else { "debug" };
    let use_color = color_enabled();

    verify_c_compiler_sanity(&toolchain.cc, Some(&resolved.sysroot_dir))?;

    if !ensure_rust_target_installed(&rust_target)? {
        bail!(
            "rust target is not installed: {}. run: rustup target add {}",
            rust_target,
            rust_target
        );
    }

    ctx.debug(format!(
        "using sysroot {} (provider={}, profile={}, platform={})",
        resolved.sysroot_dir.display(),
        resolved.provider,
        resolved.profile,
        resolved.platform_version
    ));
    ctx.debug(format!("cache entry: {}", resolved.entry_dir.display()));
    ctx.debug(format!("rust target resolved to {}", rust_target));
    ctx.debug(format!("linker resolved to {}", toolchain.linker));
    ctx.debug(format!(
        "arch map: tizen_cli_arch={}, tizen_build_arch={}, rpm_build_arch={}",
        ctx.config.tizen_cli_arch_for(arch),
        ctx.config.tizen_build_arch_for(arch),
        ctx.config.rpm_build_arch_for(arch)
    ));
    ctx.debug(format!("cargo target-dir: {}", target_dir.display()));

    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("--target").arg(&rust_target);
    if args.release {
        cmd.arg("--release");
    }
    cmd.arg("--target-dir").arg(&target_dir);
    cmd.args(&args.cargo_args);

    ToolEnv::for_cargo_build(ctx, arch, &rust_target, &resolved.sysroot_dir).apply(&mut cmd);

    for line in render_build_context(
        use_color,
        arch,
        &resolved.profile,
        &resolved.platform_version,
        resolved.provider,
        build_profile,
        &rust_target,
        &toolchain.linker,
        &resolved.sysroot_dir,
    ) {
        ctx.info(line);
    }

    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed with status: {status}");
    }

    let artifact_dir = target_dir.join(&rust_target).join(build_profile);
    ctx.info(format!(
        "{} {}",
        cargo_status(use_color, "Artifacts"),
        artifact_dir.display()
    ));

    if let Some(package_name) = package_name_from_manifest(&ctx.workspace_root.join("Cargo.toml")) {
        let primary_binary = artifact_dir.join(&package_name);
        if primary_binary.is_file() {
            ctx.info(format!(
                "{} {}",
                cargo_status(use_color, "Binary"),
                primary_binary.display()
            ));
        }
    }

    Ok(())
}

fn render_build_context(
    use_color: bool,
    arch: Arch,
    profile: &str,
    platform_version: &str,
    provider: sysroot::provider::ProviderKind,
    build_profile: &str,
    rust_target: &str,
    linker: &str,
    sysroot_dir: &Path,
) -> Vec<String> {
    let pad = " ".repeat(15);
    let build_tag = build_profile_tag(use_color, build_profile);
    vec![
        format!(
            "{} {} {}",
            cargo_status(use_color, "Cross-compiling"),
            rust_target,
            build_tag
        ),
        format!(
            "{} {} {}, {} {}, {} {}, {} {}",
            pad,
            detail_label(use_color, "arch:"),
            arch,
            detail_label(use_color, "profile:"),
            profile,
            detail_label(use_color, "platform:"),
            platform_version,
            detail_label(use_color, "provider:"),
            provider,
        ),
        format!("{} {} {}", pad, detail_label(use_color, "linker:"), linker),
        format!(
            "{} {} {}",
            pad,
            detail_label(use_color, "sysroot:"),
            sysroot_dir.display()
        ),
    ]
}

fn build_profile_tag(use_color: bool, build_profile: &str) -> String {
    let code = match build_profile {
        "release" => "1;92", // bold bright green
        _ => "1;93",         // bold bright yellow for debug
    };
    colorize(use_color, code, &format!("[{}]", build_profile))
}

fn detail_label(use_color: bool, label: &str) -> String {
    colorize(use_color, "2", label)
}

pub fn default_target_dir(workspace_root: &Path, arch: Arch) -> PathBuf {
    workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join("cargo")
}

pub fn resolve_target_dir(workspace_root: &Path, arch: Arch, explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_target_dir(workspace_root, arch))
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: Option<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
}

fn package_name_from_manifest(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    match basic_toml::from_str::<CargoManifest>(&raw) {
        Ok(parsed) => parsed.package.map(|pkg| pkg.name),
        Err(e) => {
            eprintln!("warning: failed to parse {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::render_build_context;
    use crate::arch::Arch;
    use crate::sysroot::provider::ProviderKind;

    #[test]
    fn build_context_lists_key_resolved_inputs() {
        let lines = render_build_context(
            false,
            Arch::Aarch64,
            "mobile",
            "10.0",
            ProviderKind::Rootstrap,
            "release",
            "aarch64-unknown-linux-gnu",
            "aarch64-linux-gnu-gcc",
            Path::new("/sysroot"),
        );

        let rendered = lines.join("\n");
        assert!(rendered.contains("Cross-compiling"));
        assert!(rendered.contains("aarch64-unknown-linux-gnu"));
        assert!(rendered.contains("[release]"));
        assert!(rendered.contains("arch:"));
        assert!(rendered.contains("aarch64"));
        assert!(rendered.contains("profile:"));
        assert!(rendered.contains("mobile"));
        assert!(rendered.contains("platform:"));
        assert!(rendered.contains("10.0"));
        assert!(rendered.contains("provider:"));
        assert!(rendered.contains("rootstrap"));
        assert!(rendered.contains("linker:"));
        assert!(rendered.contains("aarch64-linux-gnu-gcc"));
        assert!(rendered.contains("sysroot:"));
        assert!(rendered.contains("/sysroot"));
    }

    use super::{default_target_dir, package_name_from_manifest, resolve_target_dir};
    use std::fs;

    #[test]
    fn default_target_dir_builds_correct_path() {
        let root = Path::new("/workspace");
        assert_eq!(
            default_target_dir(root, Arch::Armv7l),
            Path::new("/workspace/target/tizen/armv7l/cargo")
        );
        assert_eq!(
            default_target_dir(root, Arch::Aarch64),
            Path::new("/workspace/target/tizen/aarch64/cargo")
        );
    }

    #[test]
    fn resolve_target_dir_uses_explicit_when_provided() {
        let root = Path::new("/workspace");
        let explicit = Path::new("/custom/target");
        assert_eq!(
            resolve_target_dir(root, Arch::Armv7l, Some(explicit)),
            Path::new("/custom/target")
        );
    }

    #[test]
    fn resolve_target_dir_falls_back_to_default() {
        let root = Path::new("/workspace");
        assert_eq!(
            resolve_target_dir(root, Arch::Aarch64, None),
            default_target_dir(root, Arch::Aarch64)
        );
    }

    #[test]
    fn package_name_from_manifest_reads_name() {
        let dir = std::env::temp_dir().join(format!("ct-manifest-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(
            package_name_from_manifest(&manifest),
            Some("my-app".to_string())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_name_from_manifest_returns_none_for_workspace() {
        let dir = std::env::temp_dir().join(format!("ct-manifest-ws-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("Cargo.toml");
        fs::write(&manifest, "[workspace]\nmembers = [\"a\"]\n").unwrap();
        assert_eq!(package_name_from_manifest(&manifest), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_name_from_manifest_returns_none_for_missing() {
        assert_eq!(
            package_name_from_manifest(Path::new("/nonexistent/Cargo.toml")),
            None
        );
    }
}
