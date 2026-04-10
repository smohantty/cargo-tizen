use anyhow::{Result, bail};

use crate::arch_detect;
use crate::cargo_runner;
use crate::cli::{BuildArgs, RpmArgs};
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};
use crate::package_select;
use crate::packaging::PackagingLayout;
use crate::rust_target;

mod rpmbuild;
mod stage;

pub fn run_rpm(ctx: &AppContext, args: &RpmArgs) -> Result<()> {
    let arch = arch_detect::resolve_arch(ctx, args.arch, "rpm")?;
    let rust_target = rust_target::resolve_for_arch(ctx, arch)?;
    let build_target_dir = cargo_runner::resolve_target_dir(&ctx.workspace_root, arch, None);
    let packages = package_select::resolve_rpm_packages(ctx, args.package.as_deref())?;
    let packaging_root = args
        .packaging_dir
        .clone()
        .or_else(|| ctx.config.packaging_dir());
    let packaging = PackagingLayout::new(&ctx.workspace_root, packaging_root.as_deref());

    // Validate authored packaging inputs before starting a potentially expensive build.
    let spec_name = ctx.config.rpm_spec_name().unwrap_or(&packages[0].name);
    let spec_path = packaging.resolve_rpm_spec(spec_name)?;
    let extra_sources_dir = packaging.rpm_sources_dir()?;

    let is_multi = packages.len() > 1;
    if is_multi {
        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        ctx.info(format!(
            "multi-package RPM: staging {} binaries [{}]",
            packages.len(),
            names.join(", ")
        ));
    }

    if !args.no_build {
        let mut cargo_args = Vec::new();
        for pkg in &packages {
            if pkg.source.requires_cargo_package_arg() {
                cargo_args.extend(["-p".to_string(), pkg.name.clone()]);
            }
        }
        let build_args = BuildArgs {
            arch: Some(arch),
            release: args.release,
            target_dir: Some(build_target_dir.clone()),
            cargo_args,
        };
        cargo_runner::run_build(ctx, &build_args)?;
    }

    let stage_output = stage::stage_binaries_from_target_dir(
        &ctx.workspace_root,
        arch,
        &rust_target,
        &build_target_dir,
        args.release,
        &packages,
    )?;
    ctx.debug(format!(
        "staging root: {}",
        stage_output.stage_root.display()
    ));

    let profile_dir = if args.release { "release" } else { "debug" };
    let rpm_arch = ctx.config.rpm_build_arch_for(arch);

    let binary_names: Vec<&str> = stage_output
        .package_names
        .iter()
        .map(|s| s.as_str())
        .collect();
    let extra_sources = match extra_sources_dir {
        Some(dir) => {
            let sources = rpmbuild::collect_extra_sources(&dir, &binary_names)?;
            if !sources.is_empty() {
                ctx.info(format!(
                    "found {} extra RPM source(s) in {}",
                    sources.len(),
                    dir.display()
                ));
            }
            sources
        }
        None => Vec::new(),
    };

    let rpms = rpmbuild::build_rpm(
        ctx,
        &ctx.workspace_root,
        &rpm_arch,
        arch,
        profile_dir,
        &spec_path,
        &stage_output.staged_binaries,
        &extra_sources,
        args.output.as_deref(),
    )?;

    if rpms.is_empty() {
        bail!("rpmbuild reported success but no RPM files were found");
    }

    let use_color = color_enabled();
    for rpm in rpms {
        ctx.info(format!(
            "{} {}",
            cargo_status(use_color, "Generated RPM"),
            rpm.display()
        ));
    }
    Ok(())
}
