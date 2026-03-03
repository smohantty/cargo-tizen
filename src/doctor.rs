use std::collections::BTreeMap;
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

    if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
        report_rootstrap_coverage(ctx, &arches, &mut failures, &mut warnings);
    }

    for arch in &arches {
        let arch = *arch;
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
                match sysroot::resolve_profile_platform_for_arch(ctx, arch) {
                    Ok(value) => value,
                    Err(err) => {
                        failures.push(format!(
                            "failed to resolve selected profile/platform for arch {}: {}",
                            arch, err
                        ));
                        continue;
                    }
                };

            let req = SetupRequest {
                arch,
                profile,
                platform_version,
                sdk_root_override,
            };
            match rootstrap::resolve_rootstrap(&req) {
                Ok(resolved) => {
                    let mut status = format!(
                        "[ok] selected rootstrap resolved: {} ({})",
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

#[derive(Debug, Clone)]
struct CoverageArchEntry {
    rootstrap_id: String,
    rootstrap_path: String,
    used_fallback: bool,
    selected: bool,
    cached: bool,
}

fn report_rootstrap_coverage(
    ctx: &AppContext,
    arches: &[Arch],
    failures: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let sdk_root_override = ctx.config.sdk_root();
    let mut grouped: BTreeMap<(String, String), BTreeMap<Arch, CoverageArchEntry>> =
        BTreeMap::new();

    for arch in arches.iter().copied() {
        let selected = match sysroot::resolve_profile_platform_for_arch(ctx, arch) {
            Ok(value) => Some(value),
            Err(err) => {
                failures.push(format!(
                    "failed to resolve selected profile/platform for arch {}: {}",
                    arch, err
                ));
                None
            }
        };

        let options =
            match rootstrap::installed_rootstrap_options(sdk_root_override.as_deref(), arch) {
                Ok(options) => options,
                Err(err) => {
                    failures.push(format!(
                        "failed to discover installed rootstrap targets for arch {}: {}",
                        arch, err
                    ));
                    continue;
                }
            };

        if options.is_empty() {
            warnings.push(format!(
                "no installed rootstrap targets discovered in SDK for arch {}",
                arch
            ));
            continue;
        }

        for option in options {
            let req = SetupRequest {
                arch,
                profile: option.profile.clone(),
                platform_version: option.platform_version.clone(),
                sdk_root_override: sdk_root_override.clone(),
            };
            let resolved = match rootstrap::resolve_rootstrap(&req) {
                Ok(resolved) => resolved,
                Err(err) => {
                    failures.push(format!(
                        "rootstrap resolve failed for arch {} profile={} platform-version={}: {}",
                        arch, option.profile, option.platform_version, err
                    ));
                    continue;
                }
            };

            let cached = match sysroot::cache_ready_for_target(
                ctx,
                arch,
                &option.profile,
                &option.platform_version,
            ) {
                Ok(value) => value,
                Err(err) => {
                    failures.push(format!(
                        "failed to inspect cache status for arch {} profile={} platform-version={}: {}",
                        arch, option.profile, option.platform_version, err
                    ));
                    false
                }
            };

            let selected_for_arch = selected
                .as_ref()
                .map(|(profile, platform_version)| {
                    option.profile == *profile && option.platform_version == *platform_version
                })
                .unwrap_or(false);

            grouped
                .entry((option.platform_version.clone(), option.profile.clone()))
                .or_default()
                .insert(
                    arch,
                    CoverageArchEntry {
                        rootstrap_id: resolved.id,
                        rootstrap_path: resolved.root_path.display().to_string(),
                        used_fallback: resolved.used_fallback,
                        selected: selected_for_arch,
                        cached,
                    },
                );
        }
    }

    if grouped.is_empty() {
        return;
    }

    ctx.info("[ok] installed rootstrap coverage by platform/profile:");
    let mut keys = grouped.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|a, b| {
        version_sort_key(&b.0)
            .cmp(&version_sort_key(&a.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    for key in keys {
        ctx.info(format!(
            "  - --platform-version {} --profile {}",
            key.0, key.1
        ));
        if let Some(arch_entries) = grouped.get(&key) {
            for arch in arches.iter().copied() {
                if let Some(entry) = arch_entries.get(&arch) {
                    let fallback_tag = if entry.used_fallback {
                        " [fallback]"
                    } else {
                        ""
                    };
                    let selected_tag = if entry.selected { " [selected]" } else { "" };
                    let cache_tag = if entry.cached {
                        " [cached]"
                    } else {
                        " [not-cached]"
                    };
                    ctx.info(format!(
                        "      {}: {} ({}){}{}{}",
                        arch,
                        entry.rootstrap_id,
                        entry.rootstrap_path,
                        fallback_tag,
                        selected_tag,
                        cache_tag
                    ));
                } else {
                    ctx.info(format!("      {}: [not-installed]", arch));
                }
            }
        }
    }
}

fn version_sort_key(version: &str) -> (u64, u64) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    (major, minor)
}

fn binary_exists(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') {
        return Path::new(value).is_file();
    }
    which::which(value).is_ok()
}
