use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::arch::Arch;
use crate::arch_detect;
use crate::cli::BuildArgs;
use crate::context::AppContext;
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
        arch,
        &resolved.profile,
        &resolved.platform_version,
        resolved.provider,
        build_profile,
        &rust_target,
        &toolchain.linker,
        &resolved.sysroot_dir,
        &target_dir,
    ) {
        ctx.info(line);
    }

    ctx.info(format!("running cargo build for {rust_target}"));

    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed with status: {status}");
    }

    let artifact_dir = target_dir.join(&rust_target).join(build_profile);
    ctx.info(format!("[ok] build artifacts: {}", artifact_dir.display()));

    if let Some(package_name) = package_name_from_manifest(&ctx.workspace_root.join("Cargo.toml")) {
        let primary_binary = artifact_dir.join(&package_name);
        if primary_binary.is_file() {
            ctx.info(format!("[ok] primary binary: {}", primary_binary.display()));
        }
    }

    Ok(())
}

fn render_build_context(
    arch: Arch,
    profile: &str,
    platform_version: &str,
    provider: sysroot::provider::ProviderKind,
    build_profile: &str,
    rust_target: &str,
    linker: &str,
    sysroot_dir: &Path,
    target_dir: &Path,
) -> Vec<String> {
    vec![
        "build context:".to_string(),
        format!(
            "  tizen: profile={} platform-version={} arch={}",
            profile, platform_version, arch
        ),
        format!("  rust-target: {}  build: {}", rust_target, build_profile),
        format!("  provider: {}  linker: {}", provider, linker),
        format!("  sysroot: {}", sysroot_dir.display()),
        format!("  target-dir: {}", target_dir.display()),
    ]
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
    match toml::from_str::<CargoManifest>(&raw) {
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
            Arch::Aarch64,
            "mobile",
            "10.0",
            ProviderKind::Rootstrap,
            "release",
            "aarch64-unknown-linux-gnu",
            "aarch64-linux-gnu-gcc",
            Path::new("/sysroot"),
            Path::new("/target/tizen/aarch64/cargo"),
        );

        let rendered = lines.join("\n");
        assert!(rendered.contains("build context:"));
        assert!(rendered.contains("profile=mobile"));
        assert!(rendered.contains("platform-version=10.0"));
        assert!(rendered.contains("arch=aarch64"));
        assert!(rendered.contains("rust-target: aarch64-unknown-linux-gnu"));
        assert!(rendered.contains("build: release"));
        assert!(rendered.contains("provider: rootstrap"));
        assert!(rendered.contains("linker: aarch64-linux-gnu-gcc"));
        assert!(rendered.contains("sysroot: /sysroot"));
        assert!(rendered.contains("target-dir: /target/tizen/aarch64/cargo"));
    }
}
