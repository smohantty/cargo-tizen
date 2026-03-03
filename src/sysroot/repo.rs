use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::sysroot::provider::{ProviderKind, SetupRequest, SysrootProvider};

pub struct RepoProvider;

impl SysrootProvider for RepoProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Repo
    }

    fn fingerprint(&self, req: &SetupRequest) -> Result<String> {
        Ok(format!(
            "repo-{}-{}-{}-v1",
            req.profile, req.platform_version, req.arch
        ))
    }

    fn prepare(&self, req: &SetupRequest, sysroot_dir: &Path) -> Result<()> {
        let stamp = format!(
            "provider=repo\narch={}\nprofile={}\nplatform_version={}\n",
            req.arch, req.profile, req.platform_version
        );
        fs::create_dir_all(sysroot_dir).with_context(|| {
            format!(
                "failed to create repo provider working directory {}",
                sysroot_dir.display()
            )
        })?;
        fs::write(sysroot_dir.join("sysroot.stamp"), stamp).with_context(|| {
            format!(
                "failed to write repo provider marker file in {}",
                sysroot_dir.display()
            )
        })?;

        bail!("repo provider is not implemented yet. use --provider rootstrap")
    }
}
