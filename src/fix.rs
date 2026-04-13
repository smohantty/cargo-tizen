use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::cli::FixArgs;
use crate::cli::SetupArgs;
use crate::context::AppContext;
use crate::output::{Section, color_enabled, print_report};
use crate::rust_target;
use crate::sysroot;
use crate::tool_env::ensure_rust_target_installed;

pub fn run_fix(ctx: &AppContext, args: &FixArgs) -> Result<()> {
    if which::which("rustup").is_err() {
        bail!("rustup is not installed or not in PATH");
    }

    let use_color = color_enabled();
    let mut sections = Vec::new();

    // -- Prerequisites -------------------------------------------------------

    let mut prereq = Section::new("Prerequisites");
    prereq.ok("rustup");
    if which::which("rpmbuild").is_ok() {
        prereq.ok("rpmbuild");
    } else {
        prereq.warn("rpmbuild not found (sudo apt install rpm) — only needed for cargo tizen rpm");
    }
    sections.push(prereq);

    // -- Per-architecture fix ------------------------------------------------

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    for arch in arches {
        let mut sec = Section::new(format!("Architecture: {arch}"));

        // Rust target
        let rust_target = match rust_target::resolve_for_arch(ctx, arch) {
            Ok(target) => target,
            Err(err) => {
                sec.error(format!("failed to resolve rust target: {err}"));
                sections.push(sec);
                continue;
            }
        };

        match ensure_rust_target_installed(&rust_target) {
            Ok(true) => sec.ok(format!("rust target installed: {rust_target}")),
            Ok(false) => {
                ctx.info(format!("installing rust target: {rust_target}"));
                let status = Command::new("rustup")
                    .arg("target")
                    .arg("add")
                    .arg(&rust_target)
                    .status()
                    .with_context(|| format!("failed to run rustup target add {}", rust_target));

                match status {
                    Ok(s) if s.success() => {
                        sec.ok(format!("installed rust target: {rust_target}"));
                    }
                    Ok(s) => sec.error(format!(
                        "rustup target add {rust_target} failed with status: {s}"
                    )),
                    Err(err) => sec.error(format!("{err}")),
                }
            }
            Err(err) => sec.error(format!("failed to query rust targets: {err}")),
        }

        // Sysroot
        if sysroot::resolve_for_build(ctx, arch).is_ok() {
            sec.ok(format!("sysroot ready for {arch}"));
        } else {
            ctx.info(format!("preparing sysroot for {arch}"));
            let setup = SetupArgs {
                arch: Some(arch),
                profile: None,
                platform_version: None,
                provider: None,
                sdk_root: None,
                force: false,
            };
            match sysroot::run_setup(ctx, &setup) {
                Ok(()) => sec.ok(format!("sysroot prepared for {arch}")),
                Err(err) => sec.error_multiline(&format!("sysroot setup failed for {arch}: {err}")),
            }
        }

        sections.push(sec);
    }

    // -- Render output -------------------------------------------------------

    let error_count = print_report(&sections, use_color, false, None);

    if error_count > 0 {
        let total = sections.len();
        bail!("fix encountered issues in {error_count} of {total} categories")
    }
    Ok(())
}
