use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::DoctorArgs;
use crate::context::AppContext;
use crate::sdk::TizenSdk;
use crate::sysroot;
use crate::sysroot::provider::SetupRequest;
use crate::sysroot::rootstrap::{self, MISSING_SDK_GUIDANCE};
use crate::tool_env::{ensure_rust_target_installed, find_tool_in_sdk, resolve_toolchain};

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
        failures.push("missing tool: rpmbuild (install package `rpm-build`)".to_string());
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

    if let Some(arch) = args.arch {
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

        let rust_target = ctx.config.rust_target_for(arch);
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
            let req = SetupRequest {
                arch,
                profile: ctx.config.profile(),
                platform_version: ctx.config.platform_version(),
                sdk_root_override: ctx.config.sdk_root(),
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

        if let Err(err) = sysroot::resolve_for_build(ctx, arch) {
            failures.push(format!("sysroot not ready for arch {}: {}", arch, err));
        } else {
            ctx.info(format!("[ok] sysroot cache ready for arch {}", arch));
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
