use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::arch_detect;
use crate::cli::{RunArgs, TpkArgs};
use crate::context::AppContext;
use crate::device;
use crate::packaging::PackagingLayout;
use crate::tpk;

pub fn run_run(ctx: &AppContext, args: &RunArgs) -> Result<()> {
    let device = device::resolve_target_device(ctx, args.device.as_deref())?;
    ctx.info(format!("using device {} ({})", device.id, device.model));

    let (package_path, packaged_manifest) = if let Some(path) = &args.tpk {
        if !path.is_file() {
            bail!("provided --tpk path does not exist: {}", path.display());
        }
        (path.clone(), None)
    } else {
        let selected_arch = resolve_run_arch(ctx, args, &device)?;
        let resolved_sign = args
            .sign
            .clone()
            .or_else(|| ctx.config.tpk_sign().map(String::from));
        let tpk_args = TpkArgs {
            arch: Some(selected_arch),
            cargo_release: args.cargo_release,
            no_build: args.no_build,
            packaging_dir: args.packaging_dir.clone(),
            output: args.output.clone(),
            sign: resolved_sign,
        };
        let packaged = tpk::package_tpk(ctx, &tpk_args)?;
        let chosen = choose_tpk(&packaged.tpk_files)?;
        if packaged.tpk_files.len() > 1 {
            ctx.info(format!(
                "multiple TPK artifacts found in {}. using {}",
                packaged.output_dir.display(),
                chosen.display()
            ));
        }
        (chosen, Some(packaged.manifest_path))
    };

    let app_id = resolve_app_id(ctx, args, packaged_manifest.as_deref())?;
    ctx.info(format!("resolved app id: {}", app_id));

    device::install_tpk_on_device(ctx, &device, &package_path)?;
    device::launch_app_on_device(ctx, &device, &app_id)?;
    Ok(())
}

fn resolve_run_arch(
    ctx: &AppContext,
    args: &RunArgs,
    device: &device::TizenDevice,
) -> Result<Arch> {
    if let Some(arch) = args.arch {
        return Ok(arch);
    }

    if let Some(device_arch) = device.cpu_arch.as_deref().and_then(Arch::parse) {
        ctx.info(format!(
            "auto-selected arch {} from target device {}",
            device_arch, device.id
        ));
        return Ok(device_arch);
    }

    arch_detect::resolve_arch(ctx, None, "run")
}

fn choose_tpk(paths: &[PathBuf]) -> Result<PathBuf> {
    match paths {
        [] => bail!("no TPK artifact found"),
        [single] => Ok(single.clone()),
        many => Ok(many[0].clone()),
    }
}

fn resolve_app_id(
    ctx: &AppContext,
    args: &RunArgs,
    packaged_manifest: Option<&std::path::Path>,
) -> Result<String> {
    if let Some(app_id) = &args.app_id {
        return Ok(app_id.clone());
    }

    if let Some(manifest) = packaged_manifest {
        return tpk::detect_app_id_from_manifest(manifest);
    }

    let packaging_root = args
        .packaging_dir
        .clone()
        .or_else(|| ctx.config.packaging_dir());
    let packaging = PackagingLayout::new(&ctx.workspace_root, packaging_root.as_deref());
    let manifest = packaging.resolve_tpk_manifest()?;
    return tpk::detect_app_id_from_manifest(&manifest);
}
