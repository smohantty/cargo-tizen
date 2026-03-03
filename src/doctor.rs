use std::path::Path;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::cli::DoctorArgs;
use crate::context::AppContext;
use crate::rust_target;
use crate::sdk::TizenSdk;
use crate::sysroot;
use crate::sysroot::provider::SetupRequest;
use crate::sysroot::rootstrap::{self, MISSING_SDK_GUIDANCE};
use crate::tool_env::{
    ensure_rust_target_installed, find_tool_in_sdk, resolve_toolchain, verify_c_compiler_sanity,
};

pub fn run_doctor(ctx: &AppContext, args: &DoctorArgs) -> Result<()> {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    for tool in ["cargo", "rustc", "rustup"] {
        if which::which(tool).is_ok() {
            ctx.info(format!("[ok] tool found: {tool}"));
        } else {
            failures.push(format!("missing tool: {tool}"));
        }
    }
    if which::which("rpmbuild").is_ok() {
        ctx.info("[ok] tool found: rpmbuild");
    } else {
        warnings.push(
            "missing tool: rpmbuild (install package `rpm-build`) [required only for `cargo tizen rpm`]"
                .to_string(),
        );
    }

    let sdk = TizenSdk::locate(ctx.config.sdk_root().as_deref());
    match &sdk {
        Some(sdk) => {
            ctx.info(format!(
                "[ok] Tizen SDK found: {} (flavor: {})",
                sdk.root().display(),
                sdk.flavor()
            ));

            let tizen_cli = sdk.tizen_cli();
            if tizen_cli.is_file() {
                ctx.info(format!("[ok] tizen CLI found: {}", tizen_cli.display()));
            } else {
                warnings.push(format!(
                    "tizen CLI not found at expected path: {}",
                    tizen_cli.display()
                ));
            }
        }
        None => failures.push(MISSING_SDK_GUIDANCE.to_string()),
    }

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    if args.arch.is_none() {
        ctx.info("checking all supported architectures: armv7l, aarch64");
    }

    for arch in arches {
        let toolchain = resolve_toolchain(ctx, arch);
        let linker = toolchain.linker;

        if binary_exists(&linker) {
            ctx.info(format!("[ok] linker found: {linker}"));
        } else {
            let mut message = format!("linker not found for arch {}: {}", arch, linker);
            if let Some(sdk) = &sdk {
                let default_linker = ctx.config.linker_for(arch);
                if let Some(found) = find_tool_in_sdk(sdk, &default_linker) {
                    message.push_str(&format!(
                        " (found candidate in SDK: {}; set [arch.{}].linker to this path)",
                        found.display(),
                        arch
                    ));
                }
            }
            failures.push(message);
        }

        let rust_target = match rust_target::resolve_for_arch(ctx, arch) {
            Ok(target) => target,
            Err(err) => {
                failures.push(format!(
                    "failed to resolve rust target for arch {}: {}",
                    arch, err
                ));
                continue;
            }
        };
        match ensure_rust_target_installed(&rust_target) {
            Ok(true) => ctx.info(format!("[ok] rust target installed: {rust_target}")),
            Ok(false) => failures.push(format!(
                "rust target not installed: {} (try: rustup target add {})",
                rust_target, rust_target
            )),
            Err(err) => failures.push(format!(
                "failed to query rust targets for {}: {}",
                rust_target, err
            )),
        }

        if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
            let sdk_root_override = ctx.config.sdk_root();
            let (profile, platform_version) =
                sysroot::resolve_profile_platform_for_arch(ctx, arch)?;

            match rootstrap::installed_rootstrap_options(sdk_root_override.as_deref(), arch) {
                Ok(options) if options.is_empty() => warnings.push(format!(
                    "no installed rootstrap targets discovered in SDK for arch {}",
                    arch
                )),
                Ok(options) => {
                    ctx.info(format!(
                        "[ok] installed rootstrap targets for arch {}:",
                        arch
                    ));
                    for option in options {
                        let is_selected = option.profile == profile
                            && option.platform_version == platform_version;
                        match sysroot::cache_ready_for_target(
                            ctx,
                            arch,
                            &option.profile,
                            &option.platform_version,
                        ) {
                            Ok(cache_ready) => {
                                let selected_tag = if is_selected { " [selected]" } else { "" };
                                let cache_tag = if cache_ready {
                                    " [cached]"
                                } else {
                                    " [not-cached]"
                                };
                                ctx.info(format!(
                                    "  - --platform-version {} --profile {}{}{}",
                                    option.platform_version, option.profile, selected_tag, cache_tag
                                ));
                            }
                            Err(err) => failures.push(format!(
                                "failed to inspect cache status for arch {} profile={} platform-version={}: {}",
                                arch, option.profile, option.platform_version, err
                            )),
                        }
                    }
                }
                Err(err) => failures.push(format!(
                    "failed to discover installed rootstrap targets for arch {}: {}",
                    arch, err
                )),
            }

            let req = SetupRequest {
                arch,
                profile,
                platform_version,
                sdk_root_override,
            };
            match rootstrap::resolve_rootstrap(&req) {
                Ok(resolved) => {
                    let mut status = format!(
                        "[ok] rootstrap found: {} ({})",
                        resolved.id,
                        resolved.root_path.display()
                    );
                    if resolved.used_fallback {
                        status.push_str(" [fallback]");
                    }
                    ctx.info(status);
                }
                Err(err) => failures.push(format!("rootstrap check failed: {err}")),
            }
        }

        match sysroot::resolve_for_build(ctx, arch) {
            Ok(resolved) => {
                ctx.info(format!("[ok] sysroot cache ready for arch {}", arch));
                if let Err(err) =
                    verify_c_compiler_sanity(&toolchain.cc, Some(&resolved.sysroot_dir))
                {
                    failures.push(format!(
                        "c compiler sanity check failed for arch {}: {}",
                        arch, err
                    ));
                } else {
                    ctx.info(format!(
                        "[ok] c compiler sanity check passed: {}",
                        toolchain.cc
                    ));
                }
            }
            Err(err) => failures.push(format!("sysroot not ready for arch {}: {}", arch, err)),
        }
    }

    for warning in &warnings {
        eprintln!("[warn] {warning}");
    }

    if failures.is_empty() {
        ctx.info("doctor checks passed");
        return Ok(());
    }

    for failure in &failures {
        eprintln!("[error] {failure}");
    }
    bail!("doctor found {} issue(s)", failures.len())
}

fn binary_exists(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') {
        return Path::new(value).is_file();
    }
    which::which(value).is_ok()
}
