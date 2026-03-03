use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
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
    let detailed = ctx.verbose;

    let mut missing_tools = Vec::new();
    for tool in ["cargo", "rustc", "rustup"] {
        if which::which(tool).is_ok() {
            if detailed {
                ctx.info(format!("[ok] tool found: {tool}"));
            }
        } else {
            missing_tools.push(tool.to_string());
            failures.push(format!("missing tool: {tool}"));
        }
    }
    if !detailed && missing_tools.is_empty() {
        ctx.info("[ok] required host tools found: cargo, rustc, rustup");
    }
    if which::which("rpmbuild").is_ok() {
        if detailed {
            ctx.info("[ok] tool found: rpmbuild");
        }
    } else {
        warnings.push(
            "missing tool: rpmbuild (install package `rpm-build`) [required only for `cargo tizen rpm`]"
                .to_string(),
        );
    }

    let sdk = TizenSdk::locate(ctx.config.sdk_root().as_deref());
    match &sdk {
        Some(sdk) => {
            if detailed {
                ctx.info(format!(
                    "[ok] Tizen SDK found: {} (flavor: {})",
                    sdk.root().display(),
                    sdk.flavor()
                ));
            } else {
                ctx.info(format!(
                    "[ok] Tizen SDK found: {} ({})",
                    sdk.root().display(),
                    sdk.flavor()
                ));
            }

            let tizen_cli = sdk.tizen_cli();
            if tizen_cli.is_file() {
                if detailed {
                    ctx.info(format!("[ok] tizen CLI found: {}", tizen_cli.display()));
                }
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
        let mut arch_ready = true;

        if binary_exists(&linker) {
            if detailed {
                ctx.info(format!("[ok] linker found: {linker}"));
            }
        } else {
            arch_ready = false;
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
            Ok(true) => {
                if detailed {
                    ctx.info(format!("[ok] rust target installed: {rust_target}"));
                }
            }
            Ok(false) => {
                arch_ready = false;
                failures.push(format!(
                    "rust target not installed: {} (try: rustup target add {})",
                    rust_target, rust_target
                ));
            }
            Err(err) => {
                arch_ready = false;
                failures.push(format!(
                    "failed to query rust targets for {}: {}",
                    rust_target, err
                ));
            }
        }

        let mut selected_rootstrap = None::<String>;
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
                    selected_rootstrap = Some(resolved.id.clone());
                    if detailed {
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
                }
                Err(err) => {
                    arch_ready = false;
                    failures.push(format!("rootstrap check failed: {err}"));
                }
            }
        }

        match sysroot::resolve_for_build(ctx, arch) {
            Ok(resolved) => {
                if let Err(err) =
                    verify_c_compiler_sanity(&toolchain.cc, Some(&resolved.sysroot_dir))
                {
                    arch_ready = false;
                    failures.push(format!(
                        "c compiler sanity check failed for arch {}: {}",
                        arch, err
                    ));
                } else {
                    if detailed {
                        ctx.info(format!("[ok] sysroot cache ready for arch {}", arch));
                        ctx.info(format!(
                            "[ok] c compiler sanity check passed: {}",
                            toolchain.cc
                        ));
                    }
                }
            }
            Err(err) => {
                arch_ready = false;
                failures.push(format!("sysroot not ready for arch {}: {}", arch, err));
            }
        }

        if !detailed && arch_ready {
            let rootstrap_note = selected_rootstrap
                .as_deref()
                .map(|id| format!(", rootstrap={id}"))
                .unwrap_or_default();
            ctx.info(format!(
                "[ok] {} ready: rust-target={}{}",
                arch, rust_target, rootstrap_note
            ));
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

fn report_rootstrap_coverage(
    ctx: &AppContext,
    arches: &[Arch],
    failures: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let sdk_root_override = ctx.config.sdk_root();
    let mut grouped: BTreeMap<(String, String), BTreeSet<Arch>> = BTreeMap::new();

    for arch in arches.iter().copied() {
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
            grouped
                .entry((option.platform_version.clone(), option.profile.clone()))
                .or_default()
                .insert(arch);
        }
    }

    if grouped.is_empty() {
        return;
    }

    ctx.info("[ok] installed rootstrap coverage by platform/profile:");
    let use_color = color_output_enabled();
    let mut keys = grouped.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|a, b| {
        version_sort_key(&b.0)
            .cmp(&version_sort_key(&a.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    for key in keys {
        if let Some(arch_entries) = grouped.get(&key) {
            let supported_arches = arches
                .iter()
                .copied()
                .filter(|arch| arch_entries.contains(arch))
                .map(|arch| arch.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let platform_label = colorize(
                use_color,
                "1;36",
                &format!("--platform-version {} --profile {}", key.0, key.1),
            );
            let arch_label = colorize(
                use_color,
                "1;32",
                &format!("supported arch: {}", supported_arches),
            );
            ctx.info(format!("  ✓ {} ({})", platform_label, arch_label));
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

fn color_output_enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn colorize(enabled: bool, ansi_code: &str, value: &str) -> String {
    if enabled {
        return format!("\x1b[{}m{}\x1b[0m", ansi_code, value);
    }
    value.to_string()
}

fn binary_exists(value: &str) -> bool {
    if value.contains('/') || value.contains('\\') {
        return Path::new(value).is_file();
    }
    which::which(value).is_ok()
}
