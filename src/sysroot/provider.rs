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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_display() {
        assert_eq!(ProviderKind::Rootstrap.to_string(), "rootstrap");
        assert_eq!(ProviderKind::Repo.to_string(), "repo");
    }

    #[test]
    fn provider_for_returns_correct_kind() {
        let rootstrap = provider_for(ProviderKind::Rootstrap);
        assert_eq!(rootstrap.kind(), ProviderKind::Rootstrap);

        let repo = provider_for(ProviderKind::Repo);
        assert_eq!(repo.kind(), ProviderKind::Repo);
    }
}
