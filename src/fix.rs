use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::cli::FixArgs;
use crate::context::AppContext;
use crate::sysroot;
use crate::cli::SetupArgs;
use crate::tool_env::ensure_rust_target_installed;

pub fn run_fix(ctx: &AppContext, args: &FixArgs) -> Result<()> {
    if which::which("rustup").is_err() {
        bail!("rustup is not installed or not in PATH");
    }

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    let mut missing_targets = Vec::new();
    let mut failures = Vec::new();
    for arch in arches {
        let rust_target = ctx.config.rust_target_for(arch);
        if ensure_rust_target_installed(&rust_target)? {
            ctx.info(format!("[ok] rust target already installed for {}: {}", arch, rust_target));
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
