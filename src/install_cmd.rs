use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::arch_detect;
use crate::cli::{InstallArgs, TpkArgs};
use crate::context::AppContext;
use crate::device;
use crate::tpk;

pub fn run_install(ctx: &AppContext, args: &InstallArgs) -> Result<()> {
    let device = device::resolve_target_device(ctx, args.device.as_deref())?;
    ctx.info(format!("using device {} ({})", device.id, device.model));

    let package_path = if let Some(path) = &args.tpk {
        if !path.is_file() {
            bail!("provided --tpk path does not exist: {}", path.display());
        }
        path.clone()
    } else {
        let selected_arch = resolve_install_arch(ctx, args, &device)?;
        let resolved_sign = args
            .sign
            .clone()
            .or_else(|| ctx.config.tpk_sign().map(String::from));
        let tpk_args = TpkArgs {
            arch: Some(selected_arch),
            package: args.package.clone(),
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
        chosen
    };

    device::install_tpk_on_device(ctx, &device, &package_path)?;
    Ok(())
}

fn resolve_install_arch(
    ctx: &AppContext,
    args: &InstallArgs,
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

    arch_detect::resolve_arch(ctx, None, "install")
}

fn choose_tpk(paths: &[PathBuf]) -> Result<PathBuf> {
    match paths {
        [] => bail!("no TPK artifact found"),
        [single] => Ok(single.clone()),
        many => Ok(many[0].clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::choose_tpk;
    use std::path::PathBuf;

    #[test]
    fn choose_tpk_single() {
        let paths = vec![PathBuf::from("/out/app.tpk")];
        assert_eq!(choose_tpk(&paths).unwrap(), PathBuf::from("/out/app.tpk"));
    }

    #[test]
    fn choose_tpk_multiple_picks_first() {
        let paths = vec![
            PathBuf::from("/out/a.tpk"),
            PathBuf::from("/out/b.tpk"),
        ];
        assert_eq!(choose_tpk(&paths).unwrap(), PathBuf::from("/out/a.tpk"));
    }

    #[test]
    fn choose_tpk_empty_fails() {
        let paths: Vec<PathBuf> = vec![];
        assert!(choose_tpk(&paths).is_err());
    }
}
