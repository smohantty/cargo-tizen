use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::arch::Arch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Rootstrap,
    Repo,
}

impl Display for ProviderKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderKind::Rootstrap => f.write_str("rootstrap"),
            ProviderKind::Repo => f.write_str("repo"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SetupRequest {
    pub arch: Arch,
    pub profile: String,
    pub platform_version: String,
    pub sdk_root_override: Option<PathBuf>,
}

pub trait SysrootProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn fingerprint(&self, req: &SetupRequest) -> Result<String>;
    fn prepare(&self, req: &SetupRequest, sysroot_dir: &Path) -> Result<()>;
}

pub fn provider_for(kind: ProviderKind) -> Box<dyn SysrootProvider> {
    match kind {
        ProviderKind::Rootstrap => Box::new(crate::sysroot::rootstrap::RootstrapProvider),
        ProviderKind::Repo => Box::new(crate::sysroot::repo::RepoProvider),
    }
}
