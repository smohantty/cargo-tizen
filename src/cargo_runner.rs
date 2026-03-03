use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

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
    if ctx.quiet {
        cmd.arg("--quiet");
    }
    if ctx.verbose {
        cmd.arg("--verbose");
    }
    cmd.arg("--target-dir").arg(&target_dir);
    cmd.args(&args.cargo_args);

    ToolEnv::for_cargo_build(ctx, arch, &rust_target, &resolved.sysroot_dir).apply(&mut cmd);

    ctx.info(format!(
        "running cargo build for {} using sysroot {}",
        rust_target,
        resolved.sysroot_dir.display()
    ));

    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed with status: {status}");
    }

    Ok(())
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
