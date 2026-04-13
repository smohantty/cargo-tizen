use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::arch::Arch;
use crate::sysroot::provider::ProviderKind;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub default: DefaultConfig,

    #[serde(default)]
    pub package: PackageConfig,

    #[serde(default)]
    pub arch: HashMap<String, ArchConfig>,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub sdk: SdkConfig,

    #[serde(default)]
    pub tpk: TpkConfig,

    #[serde(default, alias = "gh_release")]
    pub release: ReleaseConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefaultConfig {
    pub arch: Option<String>,
    pub profile: Option<String>,
    pub platform_version: Option<String>,
    pub provider: Option<String>,
    pub packaging_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageConfig {
    pub name: Option<String>,
    pub packages: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchConfig {
    pub rust_target: Option<String>,
    pub linker: Option<String>,
    pub cc: Option<String>,
    pub cxx: Option<String>,
    pub ar: Option<String>,
    pub tizen_cli_arch: Option<String>,
    pub tizen_build_arch: Option<String>,
    pub rpm_build_arch: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheConfig {
    pub root: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SdkConfig {
    pub root: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TpkConfig {
    pub sign: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReleaseConfig {
    pub arches: Option<Vec<String>>,
    pub tag_format: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut cfg = Self::default();

        if let Some(user_path) = user_config_path() {
            if user_path.exists() {
                cfg.merge(Self::read_file(&user_path)?);
            }
        }

        let project_path = PathBuf::from(".cargo-tizen.toml");
        if project_path.exists() {
            cfg.merge(Self::read_file(&project_path)?);
        }

        Ok(cfg)
    }

    pub fn read_path(path: &Path) -> Result<Self> {
        Self::read_file(path)
    }

    fn read_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let parsed: Self = basic_toml::from_str(&raw)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(parsed)
    }

    fn merge(&mut self, other: Self) {
        self.default.merge(other.default);
        self.package.merge(other.package);

        for (key, incoming) in other.arch {
            if let Some(existing) = self.arch.get_mut(&key) {
                existing.merge(incoming);
            } else {
                self.arch.insert(key, incoming);
            }
        }

        self.cache.merge(other.cache);
        self.sdk.merge(other.sdk);
        self.tpk.merge(other.tpk);
        self.release.merge(other.release);
    }

    pub fn profile(&self) -> String {
        self.default
            .profile
            .clone()
            .unwrap_or_else(|| "mobile".to_string())
    }

    pub fn platform_version(&self) -> String {
        self.default
            .platform_version
            .clone()
            .unwrap_or_else(|| "10.0".to_string())
    }

    pub fn default_provider(&self) -> ProviderKind {
        match self.default.provider.as_deref() {
            Some("repo") => ProviderKind::Repo,
            _ => ProviderKind::Rootstrap,
        }
    }

    pub fn packaging_dir(&self) -> Option<PathBuf> {
        self.default.packaging_dir.as_deref().map(expand_tilde)
    }

    pub fn package_names(&self) -> Option<&[String]> {
        self.package.packages()
    }

    pub fn primary_package(&self) -> Option<&str> {
        self.package_names()
            .and_then(|packages| packages.first().map(String::as_str))
    }

    pub fn rpm_spec_name(&self) -> Option<&str> {
        self.package.name().or_else(|| self.primary_package())
    }

    pub fn linker_for(&self, arch: Arch) -> String {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.linker.clone())
            .unwrap_or_else(|| arch.default_linker().to_string())
    }

    pub fn cc_for(&self, arch: Arch) -> Option<String> {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.cc.clone())
    }

    pub fn cxx_for(&self, arch: Arch) -> Option<String> {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.cxx.clone())
    }

    pub fn ar_for(&self, arch: Arch) -> Option<String> {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.ar.clone())
    }

    pub fn rust_target_for(&self, arch: Arch) -> String {
        self.rust_target_override_for(arch)
            .unwrap_or_else(|| arch.rust_target().to_string())
    }

    pub fn rust_target_override_for(&self, arch: Arch) -> Option<String> {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.rust_target.clone())
    }

    pub fn tizen_cli_arch_for(&self, arch: Arch) -> String {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.tizen_cli_arch.clone())
            .unwrap_or_else(|| arch.tizen_cli_arch().to_string())
    }

    pub fn tizen_build_arch_for(&self, arch: Arch) -> String {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.tizen_build_arch.clone())
            .unwrap_or_else(|| arch.tizen_build_arch().to_string())
    }

    pub fn rpm_build_arch_for(&self, arch: Arch) -> String {
        self.arch
            .get(arch.as_str())
            .and_then(|entry| entry.rpm_build_arch.clone())
            .unwrap_or_else(|| arch.rpm_arch().to_string())
    }

    pub fn sdk_root(&self) -> Option<PathBuf> {
        self.sdk.root.as_deref().map(expand_tilde)
    }

    pub fn cache_root(&self) -> PathBuf {
        if let Some(path) = self.cache.root.as_deref() {
            return expand_tilde(path);
        }

        if let Some(base) = dirs::cache_dir() {
            return base.join("cargo-tizen").join("sysroots");
        }

        PathBuf::from(".cache").join("cargo-tizen").join("sysroots")
    }

    pub fn tpk_sign(&self) -> Option<&str> {
        self.tpk.sign.as_deref()
    }
}

impl DefaultConfig {
    fn merge(&mut self, other: Self) {
        if other.arch.is_some() {
            self.arch = other.arch;
        }
        if other.profile.is_some() {
            self.profile = other.profile;
        }
        if other.platform_version.is_some() {
            self.platform_version = other.platform_version;
        }
        if other.provider.is_some() {
            self.provider = other.provider;
        }
        if other.packaging_dir.is_some() {
            self.packaging_dir = other.packaging_dir;
        }
    }
}

impl PackageConfig {
    fn merge(&mut self, other: Self) {
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.packages.is_some() {
            self.packages = other.packages;
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref().filter(|s| !s.is_empty())
    }

    pub fn packages(&self) -> Option<&[String]> {
        match &self.packages {
            Some(v) if !v.is_empty() => Some(v),
            _ => None,
        }
    }
}

impl ArchConfig {
    fn merge(&mut self, other: Self) {
        if other.rust_target.is_some() {
            self.rust_target = other.rust_target;
        }
        if other.linker.is_some() {
            self.linker = other.linker;
        }
        if other.cc.is_some() {
            self.cc = other.cc;
        }
        if other.cxx.is_some() {
            self.cxx = other.cxx;
        }
        if other.ar.is_some() {
            self.ar = other.ar;
        }
        if other.tizen_cli_arch.is_some() {
            self.tizen_cli_arch = other.tizen_cli_arch;
        }
        if other.tizen_build_arch.is_some() {
            self.tizen_build_arch = other.tizen_build_arch;
        }
        if other.rpm_build_arch.is_some() {
            self.rpm_build_arch = other.rpm_build_arch;
        }
    }
}

impl CacheConfig {
    fn merge(&mut self, other: Self) {
        if other.root.is_some() {
            self.root = other.root;
        }
    }
}

impl SdkConfig {
    fn merge(&mut self, other: Self) {
        if other.root.is_some() {
            self.root = other.root;
        }
    }
}

impl TpkConfig {
    fn merge(&mut self, other: Self) {
        if other.sign.is_some() {
            self.sign = other.sign;
        }
    }
}

impl ReleaseConfig {
    fn merge(&mut self, other: Self) {
        if other.arches.is_some() {
            self.arches = other.arches;
        }
        if other.tag_format.is_some() {
            self.tag_format = other.tag_format;
        }
    }
}

pub fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|root| root.join("cargo-tizen").join("config.toml"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_packages_none_when_omitted() {
        let config: Config = basic_toml::from_str("[default]\narch = \"aarch64\"\n").unwrap();
        assert!(config.package.packages.is_none());
        assert!(config.package_names().is_none());
    }

    #[test]
    fn package_packages_some_when_present() {
        let config: Config =
            basic_toml::from_str("[package]\npackages = [\"a\", \"b\"]\n").unwrap();
        assert_eq!(
            config.package.packages,
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            config.package_names(),
            Some(["a".to_string(), "b".to_string()].as_slice())
        );
        assert_eq!(config.primary_package(), Some("a"));
    }

    #[test]
    fn package_packages_empty_treated_as_unset() {
        let config: Config = basic_toml::from_str("[package]\npackages = []\n").unwrap();
        assert_eq!(config.package.packages, Some(vec![]));
        assert!(config.package_names().is_none());
        assert!(config.primary_package().is_none());
    }

    #[test]
    fn package_config_merge_project_replaces_user() {
        let mut base = PackageConfig {
            name: None,
            packages: Some(vec!["old".into()]),
        };
        let other = PackageConfig {
            name: None,
            packages: Some(vec!["new-a".into(), "new-b".into()]),
        };
        base.merge(other);
        assert_eq!(
            base.packages,
            Some(vec!["new-a".to_string(), "new-b".to_string()])
        );
    }

    #[test]
    fn package_config_merge_preserves_when_project_omits() {
        let mut base = PackageConfig {
            name: None,
            packages: Some(vec!["keep".into()]),
        };
        let other = PackageConfig::default();
        base.merge(other);
        assert_eq!(base.packages, Some(vec!["keep".to_string()]));
    }

    #[test]
    fn name_field_parsed_from_config() {
        let config: Config =
            basic_toml::from_str("[package]\nname = \"carbon\"\npackages = [\"carbon-daemon\"]\n")
                .unwrap();
        assert_eq!(config.package.name(), Some("carbon"));
        assert_eq!(config.rpm_spec_name(), Some("carbon"));
    }

    #[test]
    fn name_field_empty_treated_as_unset() {
        let config: Config =
            basic_toml::from_str("[package]\nname = \"\"\npackages = [\"a\"]\n").unwrap();
        assert!(config.package.name().is_none());
        assert_eq!(config.rpm_spec_name(), Some("a"));
    }

    #[test]
    fn rpm_spec_name_falls_back_to_primary_package() {
        let config: Config =
            basic_toml::from_str("[package]\npackages = [\"first\", \"second\"]\n").unwrap();
        assert!(config.package.name().is_none());
        assert_eq!(config.rpm_spec_name(), Some("first"));
    }

    #[test]
    fn name_merge_project_replaces_user() {
        let mut base = PackageConfig {
            name: Some("old-name".into()),
            packages: Some(vec!["a".into()]),
        };
        let other = PackageConfig {
            name: Some("new-name".into()),
            packages: None,
        };
        base.merge(other);
        assert_eq!(base.name(), Some("new-name"));
        assert_eq!(base.packages(), Some(["a".to_string()].as_slice()));
    }

    #[test]
    fn name_merge_preserves_when_project_omits() {
        let mut base = PackageConfig {
            name: Some("keep".into()),
            packages: None,
        };
        let other = PackageConfig::default();
        base.merge(other);
        assert_eq!(base.name(), Some("keep"));
    }

    #[test]
    fn profile_defaults_to_mobile() {
        let config = Config::default();
        assert_eq!(config.profile(), "mobile");
    }

    #[test]
    fn profile_uses_custom_value() {
        let config: Config = basic_toml::from_str("[default]\nprofile = \"tv\"\n").unwrap();
        assert_eq!(config.profile(), "tv");
    }

    #[test]
    fn platform_version_defaults_to_ten() {
        let config = Config::default();
        assert_eq!(config.platform_version(), "10.0");
    }

    #[test]
    fn platform_version_uses_custom_value() {
        let config: Config =
            basic_toml::from_str("[default]\nplatform_version = \"9.0\"\n").unwrap();
        assert_eq!(config.platform_version(), "9.0");
    }

    #[test]
    fn default_provider_is_rootstrap() {
        let config = Config::default();
        assert_eq!(config.default_provider(), ProviderKind::Rootstrap);
    }

    #[test]
    fn provider_repo_maps_to_repo() {
        let config: Config = basic_toml::from_str("[default]\nprovider = \"repo\"\n").unwrap();
        assert_eq!(config.default_provider(), ProviderKind::Repo);
    }

    #[test]
    fn provider_unknown_falls_back_to_rootstrap() {
        let config: Config = basic_toml::from_str("[default]\nprovider = \"custom\"\n").unwrap();
        assert_eq!(config.default_provider(), ProviderKind::Rootstrap);
    }

    #[test]
    fn linker_for_uses_default_when_unconfigured() {
        let config = Config::default();
        assert_eq!(
            config.linker_for(crate::arch::Arch::Armv7l),
            "arm-linux-gnueabi-gcc"
        );
        assert_eq!(
            config.linker_for(crate::arch::Arch::Aarch64),
            "aarch64-linux-gnu-gcc"
        );
    }

    #[test]
    fn linker_for_uses_config_override() {
        let config: Config =
            basic_toml::from_str("[arch.aarch64]\nlinker = \"custom-gcc\"\n").unwrap();
        assert_eq!(config.linker_for(crate::arch::Arch::Aarch64), "custom-gcc");
    }

    #[test]
    fn cc_for_returns_none_when_unconfigured() {
        let config = Config::default();
        assert!(config.cc_for(crate::arch::Arch::Armv7l).is_none());
    }

    #[test]
    fn cc_for_returns_config_value() {
        let config: Config = basic_toml::from_str("[arch.armv7l]\ncc = \"arm-cc\"\n").unwrap();
        assert_eq!(
            config.cc_for(crate::arch::Arch::Armv7l),
            Some("arm-cc".to_string())
        );
    }

    #[test]
    fn rust_target_for_uses_default_when_unconfigured() {
        let config = Config::default();
        assert_eq!(
            config.rust_target_for(crate::arch::Arch::Aarch64),
            "aarch64-unknown-linux-gnu"
        );
    }

    #[test]
    fn rust_target_override_for_returns_none_when_unconfigured() {
        let config = Config::default();
        assert!(
            config
                .rust_target_override_for(crate::arch::Arch::Aarch64)
                .is_none()
        );
    }

    #[test]
    fn rust_target_override_for_returns_config_value() {
        let config: Config =
            basic_toml::from_str("[arch.armv7l]\nrust_target = \"custom-target\"\n").unwrap();
        assert_eq!(
            config.rust_target_override_for(crate::arch::Arch::Armv7l),
            Some("custom-target".to_string())
        );
    }

    #[test]
    fn tpk_sign_returns_none_when_unconfigured() {
        let config = Config::default();
        assert!(config.tpk_sign().is_none());
    }

    #[test]
    fn tpk_sign_returns_value() {
        let config: Config = basic_toml::from_str("[tpk]\nsign = \"my_profile\"\n").unwrap();
        assert_eq!(config.tpk_sign(), Some("my_profile"));
    }

    #[test]
    fn rpm_spec_name_returns_none_when_all_absent() {
        let config = Config::default();
        assert!(config.rpm_spec_name().is_none());
    }

    #[test]
    fn default_config_merge_replaces_fields() {
        let mut base = DefaultConfig {
            arch: Some("arm".into()),
            profile: Some("mobile".into()),
            platform_version: Some("9.0".into()),
            provider: None,
            packaging_dir: None,
        };
        let other = DefaultConfig {
            arch: None,
            profile: Some("tv".into()),
            platform_version: Some("10.0".into()),
            provider: Some("repo".into()),
            packaging_dir: None,
        };
        base.merge(other);
        assert_eq!(base.arch, Some("arm".into())); // not replaced because other is None
        assert_eq!(base.profile, Some("tv".into())); // replaced
        assert_eq!(base.platform_version, Some("10.0".into())); // replaced
        assert_eq!(base.provider, Some("repo".into())); // replaced
    }

    #[test]
    fn arch_config_merge_replaces_set_fields() {
        let mut base = ArchConfig {
            linker: Some("old-gcc".into()),
            cc: Some("old-cc".into()),
            ..Default::default()
        };
        let other = ArchConfig {
            linker: Some("new-gcc".into()),
            ..Default::default()
        };
        base.merge(other);
        assert_eq!(base.linker, Some("new-gcc".into()));
        assert_eq!(base.cc, Some("old-cc".into())); // preserved
    }

    #[test]
    fn full_config_merge_combines_arch_entries() {
        let mut base = Config::default();
        base.arch.insert(
            "armv7l".into(),
            ArchConfig {
                linker: Some("old-gcc".into()),
                ..Default::default()
            },
        );
        let mut other = Config::default();
        other.arch.insert(
            "armv7l".into(),
            ArchConfig {
                cc: Some("new-cc".into()),
                ..Default::default()
            },
        );
        other.arch.insert(
            "aarch64".into(),
            ArchConfig {
                linker: Some("aarch64-gcc".into()),
                ..Default::default()
            },
        );
        base.merge(other);
        // armv7l: merged (linker preserved, cc added)
        let armv7l = base.arch.get("armv7l").unwrap();
        assert_eq!(armv7l.linker, Some("old-gcc".into()));
        assert_eq!(armv7l.cc, Some("new-cc".into()));
        // aarch64: inserted fresh
        let aarch64 = base.arch.get("aarch64").unwrap();
        assert_eq!(aarch64.linker, Some("aarch64-gcc".into()));
    }

    #[test]
    fn release_config_gh_release_alias_works() {
        let config: Config =
            basic_toml::from_str("[gh_release]\ntag_format = \"release-{version}\"\n").unwrap();
        assert_eq!(
            config.release.tag_format,
            Some("release-{version}".to_string())
        );
    }

    #[test]
    fn sdk_root_returns_none_when_unconfigured() {
        let config = Config::default();
        assert!(config.sdk_root().is_none());
    }

    #[test]
    fn packaging_dir_returns_none_when_unconfigured() {
        let config = Config::default();
        assert!(config.packaging_dir().is_none());
    }

    #[test]
    fn expand_tilde_plain_path_unchanged() {
        assert_eq!(expand_tilde("/some/path"), PathBuf::from("/some/path"));
    }
}
