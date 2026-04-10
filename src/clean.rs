use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::arch::Arch;
use crate::cli::CleanArgs;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};

pub fn run_clean(ctx: &AppContext, args: &CleanArgs) -> Result<()> {
    let mut clean_build = args.build;
    let mut clean_sysroot = args.sysroot;
    if args.all {
        clean_build = true;
        clean_sysroot = true;
    }
    if !clean_build && !clean_sysroot {
        clean_build = true;
    }

    if clean_build {
        clean_build_outputs(ctx, args)?;
    }
    if clean_sysroot {
        clean_sysroots(ctx, args)?;
    }

    Ok(())
}

fn clean_build_outputs(ctx: &AppContext, args: &CleanArgs) -> Result<()> {
    let target_root = ctx.workspace_root.join("target");
    let packaging_root = ctx
        .config
        .packaging_dir()
        .unwrap_or_else(|| ctx.workspace_root.join("tizen"));
    let use_color = color_enabled();

    if let Some(arch) = args.arch {
        if target_root.exists() {
            let tizen_dir = target_root.join("tizen");
            remove_if_exists(&target_root.join(arch.rust_target()))?;
            remove_if_exists(&tizen_dir.join(arch.as_str()))?;
        }
        remove_if_exists(&packaging_root.join(arch.as_str()))?;
        ctx.info(format!(
            "{} build outputs for arch {}",
            cargo_status(use_color, "Removed"),
            arch
        ));
        return Ok(());
    }

    if target_root.exists() {
        remove_if_exists(&target_root.join("tizen"))?;
    }
    // Remove staged artifact arch dirs but preserve authored files (rpm/, tpk/, etc.)
    for arch in &[Arch::Armv7l, Arch::Aarch64] {
        remove_if_exists(&packaging_root.join(arch.as_str()))?;
    }
    ctx.info(format!(
        "{} target/tizen build outputs and staged artifacts",
        cargo_status(use_color, "Removed")
    ));
    Ok(())
}

fn clean_sysroots(ctx: &AppContext, args: &CleanArgs) -> Result<()> {
    let cache_root = ctx.config.cache_root();
    if !cache_root.exists() {
        return Ok(());
    }

    let use_color = color_enabled();
    if let Some(arch) = args.arch {
        remove_arch_entries(&cache_root, arch.as_str())?;
        ctx.info(format!(
            "{} sysroot cache entries for arch {}",
            cargo_status(use_color, "Removed"),
            arch
        ));
        return Ok(());
    }

    remove_if_exists(&cache_root)?;
    ctx.info(format!(
        "{} all sysroot cache entries ({})",
        cargo_status(use_color, "Removed"),
        cache_root.display()
    ));
    Ok(())
}

fn remove_arch_entries(cache_root: &Path, arch: &str) -> Result<()> {
    for profile in fs::read_dir(cache_root)
        .with_context(|| format!("failed to list cache root {}", cache_root.display()))?
    {
        let profile = profile?;
        if !profile.path().is_dir() {
            continue;
        }

        for platform_version in fs::read_dir(profile.path())? {
            let platform_version = platform_version?;
            if !platform_version.path().is_dir() {
                continue;
            }

            let arch_dir = platform_version.path().join(arch);
            remove_if_exists(&arch_dir)?;
        }
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    }
    Ok(())
}
