use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

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
    let requested = req.profile.trim().to_ascii_lowercase();

    if requested == "common" {
        if version_ge(&req.platform_version, 8, 0) {
            return "tizen".to_string();
        }
        return "iot-headed".to_string();
    }

    if requested == "tv" {
        return "tv-samsung".to_string();
    }

    if requested == "mobile" && version_ge(&req.platform_version, 8, 0) {
        return "tizen".to_string();
    }

    requested
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
    let resolved = fs::canonicalize(source)
        .with_context(|| format!("failed to resolve symlink {}", source.display()))?;
    if resolved.is_dir() {
        copy_dir_recursive(&resolved, target)
    } else {
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

    use super::{candidate_ids, canonical_profile};

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
}
