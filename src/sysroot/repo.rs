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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::Arch;
    use crate::sysroot::provider::{SetupRequest, SysrootProvider};

    fn test_request() -> SetupRequest {
        SetupRequest {
            arch: Arch::Aarch64,
            profile: "mobile".into(),
            platform_version: "10.0".into(),
            sdk_root_override: None,
        }
    }

    #[test]
    fn repo_provider_kind() {
        let provider = RepoProvider;
        assert_eq!(provider.kind(), ProviderKind::Repo);
    }

    #[test]
    fn repo_provider_fingerprint_format() {
        let provider = RepoProvider;
        let fp = provider.fingerprint(&test_request()).unwrap();
        assert_eq!(fp, "repo-mobile-10.0-aarch64-v1");
    }

    #[test]
    fn repo_provider_prepare_always_fails() {
        let provider = RepoProvider;
        let dir = std::env::temp_dir().join(format!("ct-repo-test-{}", std::process::id()));
        let sysroot = dir.join("sysroot");
        let err = provider.prepare(&test_request(), &sysroot).unwrap_err();
        assert!(err.to_string().contains("not implemented yet"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
