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
    pub arch: HashMap<String, ArchConfig>,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub rpm: RpmConfig,

    #[serde(default)]
    pub sdk: SdkConfig,

    #[serde(default)]
    pub tpk: TpkConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DefaultConfig {
    pub arch: Option<String>,
    pub package: Option<String>,
    pub profile: Option<String>,
    pub platform_version: Option<String>,
    pub provider: Option<String>,
    pub packaging_dir: Option<String>,
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
pub struct RpmConfig {
    pub packager: Option<String>,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SdkConfig {
    pub root: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TpkConfig {
    pub sign: Option<String>,
}

impl Config {
    pub fn load(explicit_path: Option<&Path>) -> Result<Self> {
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

        if let Some(path) = explicit_path {
            cfg.merge(Self::read_file(path)?);
        }

        Ok(cfg)
    }

    fn read_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let parsed: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Ok(parsed)
    }

    fn merge(&mut self, other: Self) {
        self.default.merge(other.default);

        for (key, incoming) in other.arch {
            if let Some(existing) = self.arch.get_mut(&key) {
                existing.merge(incoming);
            } else {
                self.arch.insert(key, incoming);
            }
        }

        self.cache.merge(other.cache);
        self.rpm.merge(other.rpm);
        self.sdk.merge(other.sdk);
        self.tpk.merge(other.tpk);
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

    pub fn default_package(&self) -> Option<&str> {
        self.default.package.as_deref()
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
        if other.package.is_some() {
            self.package = other.package;
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

impl RpmConfig {
    fn merge(&mut self, other: Self) {
        if other.packager.is_some() {
            self.packager = other.packager;
        }
        if other.license.is_some() {
            self.license = other.license;
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
