use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::cli::DoctorArgs;
use crate::context::AppContext;
use crate::output::{Section, color_enabled, print_report};
use crate::package_select::{self, ManifestKind};
use crate::packaging::{PackagingLayout, RpmSpecStatus};
use crate::rust_target;
use crate::sdk::TizenSdk;
use crate::sysroot;
use crate::sysroot::provider::SetupRequest;
use crate::sysroot::rootstrap::{self, MISSING_SDK_GUIDANCE};
use crate::tool_env::{
    ensure_rust_target_installed, find_tool_in_sdk, resolve_toolchain, verify_c_compiler_sanity,
};

pub fn run_doctor(ctx: &AppContext, args: &DoctorArgs) -> Result<()> {
    let use_color = color_enabled();
    let mut sections = Vec::new();

    // -- Host tools ----------------------------------------------------------

    let mut host = Section::new("Host tools");
    let mut found_tools = Vec::new();
    let mut missing_tools = Vec::new();
    for tool in ["cargo", "rustc", "rustup"] {
        if which::which(tool).is_ok() {
            found_tools.push(tool);
        } else {
            missing_tools.push(tool);
        }
    }
    if missing_tools.is_empty() {
        host.ok(found_tools.join(", "));
    } else {
        for tool in &missing_tools {
            host.error(format!("missing: {tool}"));
        }
        if !found_tools.is_empty() {
            host.ok(found_tools.join(", "));
        }
    }
    if which::which("rpmbuild").is_ok() {
        host.ok("rpmbuild");
        // Inform user about cross-arch RPM buildarch_compat handling
        let host_arch = Arch::parse(std::env::consts::ARCH);
        let has_cross_target = Arch::all()
            .iter()
            .any(|a| host_arch.map_or(true, |h| h != *a));
        if has_cross_target {
            host.ok("cross-arch RPM: buildarch_compat will be applied automatically");
        }
    } else {
        host.warn("rpmbuild not found (sudo apt install rpm) — only needed for cargo tizen rpm");
    }
    sections.push(host);

    // -- Tizen SDK -----------------------------------------------------------

    let mut sdk_section = Section::new("Tizen SDK");
    let sdk = TizenSdk::locate(ctx.config.sdk_root().as_deref());
    match &sdk {
        Some(sdk) => {
            sdk_section.ok(format!("{} ({})", sdk.root().display(), sdk.flavor()));
            let tizen_cli = sdk.tizen_cli();
            if tizen_cli.is_file() {
                sdk_section.ok(format!("tizen CLI: {}", tizen_cli.display()));
            } else {
                sdk_section.warn(format!(
                    "tizen CLI not found at expected path: {}",
                    tizen_cli.display()
                ));
            }
        }
        None => sdk_section.error_multiline(MISSING_SDK_GUIDANCE),
    }
    sections.push(sdk_section);

    sections.push(build_packaging_section(ctx));

    // -- Rootstrap coverage --------------------------------------------------

    let arches: Vec<Arch> = if let Some(arch) = args.arch {
        vec![arch]
    } else {
        Arch::all().to_vec()
    };

    if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
        let cov_section = build_rootstrap_coverage_section(ctx, &arches);
        sections.push(cov_section);
    }

    // -- Per-architecture checks ---------------------------------------------

    for arch in &arches {
        let arch = *arch;
        let mut sec = Section::new(format!("Architecture: {arch}"));
        let toolchain = resolve_toolchain(ctx, arch);
        let linker = toolchain.linker;

        if binary_exists(&linker) {
            sec.ok(format!("linker: {linker}"));
        } else {
            let apt_pkg = arch.map().linker_apt_package;
            let mut message =
                format!("linker not found: {linker} (install with: sudo apt install {apt_pkg})");
            if let Some(sdk) = &sdk {
                let default_linker = ctx.config.linker_for(arch);
                if let Some(found) = find_tool_in_sdk(sdk, &default_linker) {
                    message.push_str(&format!(
                        " (candidate in SDK: {}; set [arch.{}].linker)",
                        found.display(),
                        arch
                    ));
                }
            }
            sec.error(message);
        }

        let rust_target = match rust_target::resolve_for_arch(ctx, arch) {
            Ok(target) => target,
            Err(err) => {
                sec.error(format!("failed to resolve rust target: {err}"));
                sections.push(sec);
                continue;
            }
        };
        match ensure_rust_target_installed(&rust_target) {
            Ok(true) => sec.ok(format!("rust target: {rust_target}")),
            Ok(false) => {
                sec.error(format!(
                    "rust target not installed: {rust_target} (try: rustup target add {rust_target})"
                ));
            }
            Err(err) => sec.error(format!("failed to query rust targets: {err}")),
        }

        if ctx.config.default_provider() == crate::sysroot::provider::ProviderKind::Rootstrap {
            let sdk_root_override = ctx.config.sdk_root();
            match sysroot::resolve_profile_platform_for_arch(ctx, arch) {
                Ok((profile, platform_version)) => {
                    let req = SetupRequest {
                        arch,
                        profile,
                        platform_version,
                        sdk_root_override,
                    };
                    match rootstrap::resolve_rootstrap(&req) {
                        Ok(resolved) => {
                            let mut msg = format!(
                                "rootstrap: {} ({})",
                                resolved.id,
                                resolved.root_path.display()
                            );
                            if resolved.used_fallback {
                                msg.push_str(" [fallback]");
                            }
                            sec.ok(msg);
                        }
                        Err(err) => sec.error_multiline(&format!("rootstrap: {err}")),
                    }
                }
                Err(err) => {
                    sec.error(format!("profile/platform resolution failed: {err}"));
                }
            }
        }

        match sysroot::resolve_for_build(ctx, arch) {
            Ok(resolved) => {
                sec.ok(format!("sysroot cache: {}", resolved.sysroot_dir.display()));
                match verify_c_compiler_sanity(&toolchain.cc, Some(&resolved.sysroot_dir)) {
                    Ok(()) => sec.ok(format!("C compiler: {}", toolchain.cc)),
                    Err(err) => {
                        let apt_pkg = arch.map().linker_apt_package;
                        sec.error(format!(
                            "C compiler sanity failed: {err} (install with: sudo apt install {apt_pkg})"
                        ));
                    }
                }
            }
            Err(err) => sec.error_multiline(&format!("sysroot: {err}")),
        }

        sections.push(sec);
    }

    // -- Render output -------------------------------------------------------

    let error_count = print_report(&sections, use_color, false, None);

    if error_count > 0 {
        let total = sections.len();
        bail!("doctor found issues in {error_count} of {total} categories")
    }
    Ok(())
}

fn build_rootstrap_coverage_section(ctx: &AppContext, arches: &[Arch]) -> Section {
    let mut sec = Section::new("Rootstrap coverage");
    let sdk_root_override = ctx.config.sdk_root();
    let mut grouped: BTreeMap<(String, String), BTreeSet<Arch>> = BTreeMap::new();
    let mut any_warning = false;

    for arch in arches.iter().copied() {
        let options =
            match rootstrap::installed_rootstrap_options(sdk_root_override.as_deref(), arch) {
                Ok(options) => options,
                Err(err) => {
                    sec.error(format!("failed to discover rootstraps for {arch}: {err}"));
                    continue;
                }
            };

        if options.is_empty() {
            sec.warn(format!("no rootstrap targets found for {arch}"));
            any_warning = true;
            continue;
        }

        for option in options {
            grouped
                .entry((option.platform_version.clone(), option.profile.clone()))
                .or_default()
                .insert(arch);
        }
    }

    if grouped.is_empty() && !any_warning && sec.items.is_empty() {
        sec.warn("no rootstrap targets discovered");
        return sec;
    }

    let mut keys = grouped.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|a, b| {
        version_sort_key(&b.0)
            .cmp(&version_sort_key(&a.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    for key in keys {
        if let Some(arch_entries) = grouped.get(&key) {
            let arches_str = arches
                .iter()
                .copied()
                .filter(|arch| arch_entries.contains(arch))
                .map(|arch| arch.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            sec.ok(format!(
                "--platform-version {} --profile {} ({})",
                key.0, key.1, arches_str
            ));
        }
    }

    sec
}

fn build_packaging_section(ctx: &AppContext) -> Section {
    let mut sec = Section::new("Packaging layout");
    let manifest_path = ctx.workspace_root.join("Cargo.toml");

    if !manifest_path.is_file() {
        sec.warn("Cargo.toml not found in the current workspace root");
        return sec;
    }

    let package_name = match ctx.config.primary_package() {
        Some(name) => {
            sec.ok(format!(
                "selected package: {} (from [package].packages)",
                name
            ));
            name.to_string()
        }
        None => match package_select::inspect_manifest(&manifest_path) {
            Ok(ManifestKind::Package(name)) => name,
            Ok(ManifestKind::Workspace) => {
                sec.warn(
                    "workspace manifest detected; set [package].packages or pass -p <member> to rpm/tpk/install",
                );
                return sec;
            }
            Ok(ManifestKind::Unknown) => {
                sec.warn("failed to determine package name from Cargo.toml");
                return sec;
            }
            Err(err) => {
                sec.warn(format!("failed to inspect Cargo.toml for packaging: {err}"));
                return sec;
            }
        },
    };

    let packaging_root = ctx.config.packaging_dir();
    let packaging = PackagingLayout::new(&ctx.workspace_root, packaging_root.as_deref());
    sec.ok(format!("packaging root: {}", packaging.root().display()));

    let spec_name = ctx.config.rpm_spec_name().unwrap_or(&package_name);
    match packaging.inspect_rpm_spec(spec_name) {
        Ok(RpmSpecStatus::Found(rpm_spec)) => {
            sec.ok(format!("rpm spec: {}", rpm_spec.display()));
        }
        Ok(RpmSpecStatus::Missing(missing)) => {
            sec.warn_multiline(&missing.doctor_message());
        }
        Err(err) => {
            sec.warn(format!("failed to inspect rpm spec layout: {err}"));
        }
    }

    let tpk_manifest = packaging.tpk_manifest_path();
    if tpk_manifest.is_file() {
        sec.ok(format!("tpk manifest: {}", tpk_manifest.display()));
    } else {
        sec.warn_multiline(&format!(
            "tpk manifest missing: {}\ngenerate with: cargo tizen init --tpk",
            tpk_manifest.display()
        ));
    }

    sec
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

#[cfg(test)]
mod tests {
    use super::version_sort_key;

    #[test]
    fn version_sort_key_parses_major_minor() {
        assert_eq!(version_sort_key("10.0"), (10, 0));
        assert_eq!(version_sort_key("9.5"), (9, 5));
    }

    #[test]
    fn version_sort_key_handles_single_component() {
        assert_eq!(version_sort_key("7"), (7, 0));
    }

    #[test]
    fn version_sort_key_handles_empty_string() {
        assert_eq!(version_sort_key(""), (0, 0));
    }

    #[test]
    fn version_sort_key_handles_non_numeric() {
        assert_eq!(version_sort_key("abc"), (0, 0));
        assert_eq!(version_sort_key("1.abc"), (1, 0));
    }

    #[test]
    fn version_sort_orders_correctly() {
        assert!(version_sort_key("10.0") > version_sort_key("9.5"));
        assert!(version_sort_key("9.5") > version_sort_key("9.0"));
        assert!(version_sort_key("10.1") > version_sort_key("10.0"));
    }
}
