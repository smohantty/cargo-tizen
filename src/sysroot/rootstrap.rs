use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::sdk::{SdkFlavor, TizenSdk};
use crate::sysroot::provider::{ProviderKind, SetupRequest, SysrootProvider};

pub struct RootstrapProvider;

#[derive(Debug, Clone)]
pub struct ResolvedRootstrap {
    pub id: String,
    pub profile: String,
    pub root_path: PathBuf,
    pub used_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRootstrapOption {
    pub platform_version: String,
    pub profile: String,
    pub rootstrap_id: String,
}

impl SysrootProvider for RootstrapProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Rootstrap
    }

    fn fingerprint(&self, req: &SetupRequest) -> Result<String> {
        let (primary, fallback) = candidate_ids(req);
        let fingerprint = if let Some(fallback) = fallback {
            format!("rootstrap-{}-fallback-{}", primary, fallback)
        } else {
            format!("rootstrap-{}", primary)
        };
        Ok(fingerprint)
    }

    fn prepare(&self, req: &SetupRequest, sysroot_dir: &Path) -> Result<()> {
        let resolved = resolve_rootstrap(req)?;
        copy_dir_recursive(&resolved.root_path, sysroot_dir)?;

        let stamp = format!(
            "provider=rootstrap\narch={}\nprofile={}\nplatform_version={}\nresolved_profile={}\nrootstrap_id={}\nrootstrap_path={}\n",
            req.arch,
            req.profile,
            req.platform_version,
            resolved.profile,
            resolved.id,
            resolved.root_path.display(),
        );
        fs::write(sysroot_dir.join("sysroot.stamp"), stamp).with_context(|| {
            format!(
                "failed to write sysroot stamp file in {}",
                sysroot_dir.display()
            )
        })?;
        Ok(())
    }
}

pub const MISSING_SDK_GUIDANCE: &str = "unable to locate Tizen SDK.\n\
Install Tizen SDK first:\n\
- tools page: https://samsungtizenos.com/tools-download/\n\
- direct SDK installer index: https://download.tizen.org/sdk/Installer/tizen-sdk_10.0/\n\
Then configure one of:\n\
- environment: TIZEN_SDK=/path/to/sdk\n\
- project/user config: [sdk].root = \"/path/to/sdk\"\n\
- setup flag: cargo tizen setup ... --sdk-root /path/to/sdk";

pub fn resolve_rootstrap(req: &SetupRequest) -> Result<ResolvedRootstrap> {
    let sdk = TizenSdk::locate(req.sdk_root_override.as_deref())
        .ok_or_else(|| anyhow::anyhow!(MISSING_SDK_GUIDANCE))?;

    let (primary_id, fallback_id) = candidate_ids(req);
    let primary_profile = canonical_profile(req);
    let primary = rootstrap_path(&sdk, req, &primary_profile, &primary_id);

    if primary.is_dir() {
        return Ok(ResolvedRootstrap {
            id: primary_id,
            profile: primary_profile,
            root_path: primary,
            used_fallback: false,
        });
    }

    if let Some(fallback_id) = fallback_id {
        let fallback_profile = fallback_profile(req).unwrap_or_else(|| primary_profile.clone());
        let fallback = rootstrap_path(&sdk, req, &fallback_profile, &fallback_id);
        if fallback.is_dir() {
            return Ok(ResolvedRootstrap {
                id: fallback_id,
                profile: fallback_profile,
                root_path: fallback,
                used_fallback: true,
            });
        }
        bail!(
            "{}",
            missing_rootstrap_message(
                req,
                &sdk,
                &primary_profile,
                &primary,
                Some((&fallback_profile, &fallback))
            )
        );
    }

    bail!(
        "{}",
        missing_rootstrap_message(req, &sdk, &primary_profile, &primary, None)
    )
}

pub fn select_installed_profile_platform(
    sdk_root_override: Option<&Path>,
    arch: Arch,
    requested_profile: Option<&str>,
    requested_platform: Option<&str>,
) -> Result<Option<InstalledRootstrapOption>> {
    let options = installed_rootstrap_options(sdk_root_override, arch)?;
    if options.is_empty() {
        return Ok(None);
    }

    let mut filtered = options.clone();
    if let Some(platform_version) = requested_platform {
        filtered.retain(|option| option.platform_version == platform_version);
    }
    if let Some(profile) = requested_profile {
        filtered.retain(|option| profile_matches(profile, option));
    }

    if filtered.is_empty() && (requested_profile.is_some() || requested_platform.is_some()) {
        let mut reason = String::from("requested rootstrap target is not installed");
        if let Some(profile) = requested_profile {
            reason.push_str(&format!(", profile={profile}"));
        }
        if let Some(platform_version) = requested_platform {
            reason.push_str(&format!(", platform-version={platform_version}"));
        }
        reason.push_str(&format!(", arch={arch}.\n"));
        reason.push_str(&format!(
            "installed options in SDK:\n{}",
            format_installed_options(&options)
        ));
        bail!("{reason}");
    }

    Ok(select_best_option(&filtered))
}

pub fn installed_rootstrap_options(
    sdk_root_override: Option<&Path>,
    arch: Arch,
) -> Result<Vec<InstalledRootstrapOption>> {
    let Some(sdk) = TizenSdk::locate(sdk_root_override) else {
        return Ok(Vec::new());
    };
    discover_installed_rootstrap_options(&sdk, arch)
}

fn discover_installed_rootstrap_options(
    sdk: &TizenSdk,
    arch: Arch,
) -> Result<Vec<InstalledRootstrapOption>> {
    let mut options = Vec::new();
    let rootstrap_type = arch.rootstrap_type();

    let platforms_dir = sdk.platforms_dir();
    if !platforms_dir.is_dir() {
        return Ok(options);
    }

    for platform_entry in fs::read_dir(&platforms_dir)
        .with_context(|| format!("failed to read {}", platforms_dir.display()))?
    {
        let platform_entry = platform_entry?;
        let platform_path = platform_entry.path();
        if !platform_path.is_dir() {
            continue;
        }

        let platform_name = platform_entry.file_name().to_string_lossy().to_string();
        let Some(platform_version) = platform_name.strip_prefix("tizen-") else {
            continue;
        };

        for profile_entry in fs::read_dir(&platform_path)
            .with_context(|| format!("failed to read {}", platform_path.display()))?
        {
            let profile_entry = profile_entry?;
            let profile_path = profile_entry.path();
            if !profile_path.is_dir() {
                continue;
            }

            let profile = profile_entry.file_name().to_string_lossy().to_string();
            let id = rootstrap_id(&profile, platform_version, rootstrap_type);
            let candidate = profile_path.join("rootstraps").join(&id);
            if candidate.is_dir() {
                options.push(InstalledRootstrapOption {
                    platform_version: platform_version.to_string(),
                    profile,
                    rootstrap_id: id,
                });
            }
        }
    }

    options.sort_by(|a, b| {
        version_sort_key(&b.platform_version)
            .cmp(&version_sort_key(&a.platform_version))
            .then_with(|| profile_rank(&a.profile).cmp(&profile_rank(&b.profile)))
            .then_with(|| a.profile.cmp(&b.profile))
    });
    options.dedup_by(|a, b| a.platform_version == b.platform_version && a.profile == b.profile);
    Ok(options)
}

fn profile_matches(requested_profile: &str, option: &InstalledRootstrapOption) -> bool {
    let primary = canonical_profile_name(requested_profile, &option.platform_version);
    if primary == option.profile {
        return true;
    }

    if primary != "tv-samsung" {
        return false;
    }

    let fallback = if version_ge(&option.platform_version, 8, 0) {
        "tizen"
    } else {
        "iot-headed"
    };
    option.profile == fallback
}

fn canonical_profile_name(profile: &str, platform_version: &str) -> String {
    let requested = profile.trim().to_ascii_lowercase();

    if requested == "common" {
        if version_ge(platform_version, 8, 0) {
            return "tizen".to_string();
        }
        return "iot-headed".to_string();
    }

    if requested == "tv" {
        return "tv-samsung".to_string();
    }

    if requested == "mobile" && version_ge(platform_version, 8, 0) {
        return "tizen".to_string();
    }

    requested
}

fn select_best_option(options: &[InstalledRootstrapOption]) -> Option<InstalledRootstrapOption> {
    options.iter().cloned().max_by(|a, b| {
        version_sort_key(&a.platform_version)
            .cmp(&version_sort_key(&b.platform_version))
            .then_with(|| profile_rank(&b.profile).cmp(&profile_rank(&a.profile)))
            .then_with(|| b.profile.cmp(&a.profile))
    })
}

fn profile_rank(profile: &str) -> usize {
    match profile {
        "tizen" => 0,
        "mobile" => 1,
        "tv-samsung" => 2,
        "wearable" => 3,
        "iot-headed" => 4,
        _ => 10,
    }
}

fn version_sort_key(version: &str) -> (u64, u64) {
    parse_version(version)
}

fn format_installed_options(options: &[InstalledRootstrapOption]) -> String {
    if options.is_empty() {
        return "  <none>".to_string();
    }

    options
        .iter()
        .map(|option| {
            format!(
                "  --platform-version {} --profile {}",
                option.platform_version, option.profile
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn candidate_ids(req: &SetupRequest) -> (String, Option<String>) {
    let profile = canonical_profile(req);
    let primary = rootstrap_id(&profile, &req.platform_version, req.arch.rootstrap_type());
    let fallback = fallback_profile(req).map(|fallback_profile| {
        rootstrap_id(
            &fallback_profile,
            &req.platform_version,
            req.arch.rootstrap_type(),
        )
    });
    (primary, fallback)
}

fn canonical_profile(req: &SetupRequest) -> String {
    canonical_profile_name(&req.profile, &req.platform_version)
}

fn fallback_profile(req: &SetupRequest) -> Option<String> {
    let primary = canonical_profile(req);
    if primary != "tv-samsung" {
        return None;
    }

    if version_ge(&req.platform_version, 8, 0) {
        Some("tizen".to_string())
    } else {
        Some("iot-headed".to_string())
    }
}

fn rootstrap_id(profile: &str, platform_version: &str, rootstrap_type: &str) -> String {
    format!("{profile}-{platform_version}-{rootstrap_type}.core")
}

fn rootstrap_path(sdk: &TizenSdk, req: &SetupRequest, profile: &str, id: &str) -> PathBuf {
    sdk.platforms_dir()
        .join(format!("tizen-{}", req.platform_version))
        .join(profile)
        .join("rootstraps")
        .join(id)
}

fn missing_rootstrap_message(
    req: &SetupRequest,
    sdk: &TizenSdk,
    profile: &str,
    primary: &Path,
    fallback: Option<(&str, &Path)>,
) -> String {
    let id = rootstrap_id(profile, &req.platform_version, req.arch.rootstrap_type());
    let mut message = format!("rootstrap {id} was not found at {}\n", primary.display());
    if let Some((fallback_profile, fallback_path)) = fallback {
        let fallback_id = rootstrap_id(
            fallback_profile,
            &req.platform_version,
            req.arch.rootstrap_type(),
        );
        message.push_str(&format!(
            "fallback rootstrap {fallback_id} was not found at {}\n",
            fallback_path.display()
        ));
    }

    if let Ok(options) = discover_installed_rootstrap_options(sdk, req.arch) {
        if !options.is_empty() {
            message.push_str("\ninstalled options for this arch:\n");
            message.push_str(&format_installed_options(&options));
            message.push('\n');
        }
    }

    match sdk.flavor() {
        SdkFlavor::Cli => {
            let install_profile = if profile == "tv-samsung" {
                fallback
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| profile.to_string())
            } else {
                profile.to_string()
            };
            let profile_pkg = install_profile.to_uppercase().replace("HEADED", "Headed");
            message.push_str(&format!(
                "install missing package(s) with:\n{} install {}-{}-NativeAppDevelopment-CLI",
                sdk.package_manager_cli().display(),
                profile_pkg,
                req.platform_version
            ));
        }
        SdkFlavor::Extension => {
            message.push_str(
                "open \"Tizen: Package Manager\" in VS Code and install Native App Development packages for this platform version",
            );
        }
    }

    message
}

fn version_ge(version: &str, major: u64, minor: u64) -> bool {
    let parsed = parse_version(version);
    parsed >= (major, minor)
}

fn parse_version(version: &str) -> (u64, u64) {
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

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        bail!(
            "source rootstrap directory does not exist: {}",
            source.display()
        );
    }
    fs::create_dir_all(target)
        .with_context(|| format!("failed to create target sysroot dir {}", target.display()))?;

    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to read source directory {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .with_context(|| format!("failed to read metadata for {}", source_path.display()))?;
        let file_type = metadata.file_type();

        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
            continue;
        }

        if file_type.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create parent directory while copying {}",
                        target_path.display()
                    )
                })?;
            }
            fs::copy(&source_path, &target_path).with_context(|| {
                format!(
                    "failed to copy file {} -> {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
            continue;
        }

        if file_type.is_symlink() {
            copy_symlink(&source_path, &target_path)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    let link = fs::read_link(source)
        .with_context(|| format!("failed to read symlink {}", source.display()))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directory while creating symlink {}",
                target.display()
            )
        })?;
    }
    symlink(&link, target).with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            target.display(),
            link.display()
        )
    })
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    let link_target = fs::read_link(source)
        .with_context(|| format!("failed to read symlink {}", source.display()))?;
    let resolved = if link_target.is_relative() {
        source.parent().unwrap_or(source).join(&link_target)
    } else {
        link_target
    };
    if !resolved.exists() {
        // Dangling symlink — skip but warn (matches Unix behavior where we
        // recreate the original link, which may also dangle).
        eprintln!(
            "warning: skipping dangling symlink {} -> {}",
            source.display(),
            resolved.display()
        );
        return Ok(());
    }
    if resolved.is_dir() {
        copy_dir_recursive(&resolved, target)
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create parent directory while copying symlink target {}",
                    target.display()
                )
            })?;
        }
        fs::copy(&resolved, target).with_context(|| {
            format!(
                "failed to copy symlink target {} -> {}",
                resolved.display(),
                target.display()
            )
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::arch::Arch;
    use crate::sysroot::provider::SetupRequest;

    use super::{
        InstalledRootstrapOption, candidate_ids, canonical_profile, profile_matches,
        select_best_option,
    };

    #[test]
    fn profile_mapping_for_common_pre_8_0() {
        let req = SetupRequest {
            arch: Arch::Armv7l,
            profile: "common".to_string(),
            platform_version: "6.0".to_string(),
            sdk_root_override: None,
        };
        assert_eq!(canonical_profile(&req), "iot-headed");
    }

    #[test]
    fn profile_mapping_for_common_post_8_0() {
        let req = SetupRequest {
            arch: Arch::Aarch64,
            profile: "common".to_string(),
            platform_version: "8.0".to_string(),
            sdk_root_override: Some(PathBuf::from("/tmp/ignore")),
        };
        let (primary, fallback) = candidate_ids(&req);
        assert_eq!(primary, "tizen-8.0-device64.core");
        assert!(fallback.is_none());
    }

    #[test]
    fn select_best_prefers_newer_platform_version() {
        let options = vec![
            InstalledRootstrapOption {
                platform_version: "9.0".to_string(),
                profile: "tizen".to_string(),
                rootstrap_id: "tizen-9.0-device.core".to_string(),
            },
            InstalledRootstrapOption {
                platform_version: "10.0".to_string(),
                profile: "tizen".to_string(),
                rootstrap_id: "tizen-10.0-device.core".to_string(),
            },
        ];
        let selected = select_best_option(&options).expect("one option should be selected");
        assert_eq!(selected.platform_version, "10.0");
    }

    #[test]
    fn tv_profile_matches_tizen_fallback_post_8_0() {
        let option = InstalledRootstrapOption {
            platform_version: "10.0".to_string(),
            profile: "tizen".to_string(),
            rootstrap_id: "tizen-10.0-device.core".to_string(),
        };
        assert!(profile_matches("tv", &option));
    }

    #[test]
    fn tv_profile_matches_iot_headed_fallback_pre_8_0() {
        let option = InstalledRootstrapOption {
            platform_version: "6.0".to_string(),
            profile: "iot-headed".to_string(),
            rootstrap_id: "iot-headed-6.0-device.core".to_string(),
        };
        assert!(profile_matches("tv", &option));
    }
}
