use anyhow::{Result, bail};

use crate::arch_detect;
use crate::cargo_runner;
use crate::cli::{BuildArgs, RpmArgs};
use crate::context::AppContext;

mod rpmbuild;
mod spec;
mod stage;

pub fn run_rpm(ctx: &AppContext, args: &RpmArgs) -> Result<()> {
    let arch = arch_detect::resolve_arch(ctx, args.arch, "rpm")?;
    let rust_target = ctx.config.rust_target_for(arch);
    let build_target_dir = cargo_runner::resolve_target_dir(&ctx.workspace_root, arch, None);

    if !args.no_build {
        let build_args = BuildArgs {
            arch: Some(arch),
            release: args.cargo_release,
            target_dir: Some(build_target_dir.clone()),
            cargo_args: Vec::new(),
        };
        cargo_runner::run_build(ctx, &build_args)?;
    }

    let stage = stage::stage_binary_from_target_dir(
        &ctx.workspace_root,
        arch,
        &rust_target,
        &build_target_dir,
        args.cargo_release,
    )?;
    ctx.debug(format!("staging root: {}", stage.stage_root.display()));

    let profile_dir = if args.cargo_release {
        "release"
    } else {
        "debug"
    };
    let rpmbuild_root = ctx
        .workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join(profile_dir)
        .join("rpmbuild");
    let rpm_arch = ctx.config.rpm_build_arch_for(arch);

    let spec_path = if let Some(path) = &args.spec {
        path.clone()
    } else {
        let generated_spec = rpmbuild_root
            .join("SPECS")
            .join(format!("{}.spec", stage.package.name));
        let input = spec::SpecInput {
            package_name: stage.package.name.clone(),
            version: stage.package.version.clone(),
            release: args.release.clone(),
            summary: format!("{} packaged by cargo-tizen", stage.package.name),
            license: ctx
                .config
                .rpm
                .license
                .clone()
                .unwrap_or_else(|| "Apache-2.0".to_string()),
            rpm_arch: rpm_arch.clone(),
            binary_name: stage.package.name.clone(),
        };
        spec::write_spec(&generated_spec, &input)?;
        generated_spec
    };

    let rpms = rpmbuild::build_rpm(
        ctx,
        &ctx.workspace_root,
        &rpm_arch,
        arch,
        profile_dir,
        &spec_path,
        &stage.staged_binary,
        &stage.package.name,
        args.output.as_deref(),
    )?;

    if rpms.is_empty() {
        bail!("rpmbuild reported success but no RPM files were found");
    }

    for rpm in rpms {
        ctx.info(format!("generated RPM: {}", rpm.display()));
    }
    Ok(())
}
