use anyhow::{Result, bail};

use crate::arch_detect;
use crate::cargo_runner;
use crate::cli::{BuildArgs, RpmArgs};
use crate::context::AppContext;
use crate::packaging::PackagingLayout;
use crate::rust_target;

mod rpmbuild;
mod stage;

pub fn run_rpm(ctx: &AppContext, args: &RpmArgs) -> Result<()> {
    let arch = arch_detect::resolve_arch(ctx, args.arch, "rpm")?;
    let rust_target = rust_target::resolve_for_arch(ctx, arch)?;
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
    let rpm_arch = ctx.config.rpm_build_arch_for(arch);
    let packaging_root = args
        .packaging_dir
        .clone()
        .or_else(|| ctx.config.packaging_dir());
    let packaging = PackagingLayout::new(&ctx.workspace_root, packaging_root.as_deref());
    let spec_path = packaging.resolve_rpm_spec(&stage.package.name)?;

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
