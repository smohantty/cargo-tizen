use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::package_select::SelectedPackage;

#[derive(Debug, Clone)]
pub struct StageOutput {
    pub stage_root: PathBuf,
    pub staged_binaries: Vec<PathBuf>,
    pub package_names: Vec<String>,
}

pub fn stage_binaries_from_target_dir(
    workspace_root: &Path,
    arch: Arch,
    rust_target: &str,
    target_dir: &Path,
    release: bool,
    packages: &[SelectedPackage],
) -> Result<StageOutput> {
    let profile_dir = if release { "release" } else { "debug" };

    // Deduplicate binary names before staging
    let mut seen = HashSet::new();
    for pkg in packages {
        if !seen.insert(&pkg.name) {
            bail!(
                "duplicate binary name `{}` in package list\n\
                 each package must produce a uniquely-named binary",
                pkg.name
            );
        }
    }

    let base = workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join(profile_dir);

    let stage_root = base.join("stage");

    // Atomic staging: build in a temp dir, then rename into place
    let stage_tmp = base.join("stage.tmp");

    // Clean temp staging dir
    if stage_tmp.exists() {
        fs::remove_dir_all(&stage_tmp).with_context(|| {
            format!(
                "failed to clean temporary staging directory {}",
                stage_tmp.display()
            )
        })?;
    }

    let bin_dir = stage_tmp.join("usr/bin");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("failed to create staging directory {}", bin_dir.display()))?;

    let mut staged_binaries = Vec::with_capacity(packages.len());
    let mut package_names = Vec::with_capacity(packages.len());

    for pkg in packages {
        let source_binary = target_dir
            .join(rust_target)
            .join(profile_dir)
            .join(&pkg.name);

        if !source_binary.exists() {
            bail!(
                "expected built binary was not found: {}\n\
                 cargo-tizen expects the binary name to match [package].name (`{}`)\n\
                 if this package uses a custom [[bin]] name or is a library-only crate,\n\
                 remove it from [package].packages",
                source_binary.display(),
                pkg.name
            );
        }

        let staged = bin_dir.join(&pkg.name);
        fs::copy(&source_binary, &staged).with_context(|| {
            format!(
                "failed to copy built binary {} -> {}",
                source_binary.display(),
                staged.display()
            )
        })?;

        staged_binaries.push(staged);
        package_names.push(pkg.name.clone());
    }

    // Assert staged count matches expected (catches silent overwrite from [[bin]] edge cases)
    let actual_count = fs::read_dir(&bin_dir)
        .with_context(|| format!("failed to verify staging directory {}", bin_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to read staging directory entries in {}",
                bin_dir.display()
            )
        })?
        .len();
    if actual_count != packages.len() {
        bail!(
            "staging integrity check failed: expected {} binaries in {} but found {}\n\
             this may indicate binary name collisions from custom [[bin]] targets",
            packages.len(),
            bin_dir.display(),
            actual_count
        );
    }

    // Crash-safe swap: rename old aside, rename new in, then remove old.
    // If interrupted between step 1 and 2, stage.old still has the prior state.
    let stage_old = base.join("stage.old");

    if stage_old.exists() {
        fs::remove_dir_all(&stage_old).ok();
    }
    if stage_root.exists() {
        fs::rename(&stage_root, &stage_old).with_context(|| {
            format!(
                "failed to move old staging directory {} -> {}",
                stage_root.display(),
                stage_old.display()
            )
        })?;
    }
    fs::rename(&stage_tmp, &stage_root).with_context(|| {
        format!(
            "failed to finalize staging directory {} -> {}",
            stage_tmp.display(),
            stage_root.display()
        )
    })?;
    if stage_old.exists() {
        fs::remove_dir_all(&stage_old).ok();
    }

    // Fix up paths to point at final location
    let staged_binaries = package_names
        .iter()
        .map(|name| stage_root.join("usr/bin").join(name))
        .collect();

    Ok(StageOutput {
        stage_root,
        staged_binaries,
        package_names,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::package_select::{PackageSource, SelectedPackage};

    fn make_package(name: &str) -> SelectedPackage {
        SelectedPackage {
            name: name.to_string(),
            source: PackageSource::Config,
        }
    }

    #[test]
    fn stage_single_binary() {
        let dir = std::env::temp_dir().join(format!("ct-stage-single-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let target_dir = dir.join("target");
        let binary_dir = target_dir.join("aarch64-unknown-linux-gnu/release");
        fs::create_dir_all(&binary_dir).unwrap();
        fs::write(binary_dir.join("my-app"), b"fake binary").unwrap();

        let packages = vec![make_package("my-app")];
        let result = stage_binaries_from_target_dir(
            &workspace,
            Arch::Aarch64,
            "aarch64-unknown-linux-gnu",
            &target_dir,
            true,
            &packages,
        )
        .unwrap();

        assert_eq!(result.package_names, vec!["my-app"]);
        assert_eq!(result.staged_binaries.len(), 1);
        assert!(result.staged_binaries[0].exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_rejects_duplicate_binary_names() {
        let dir = std::env::temp_dir().join(format!("ct-stage-dup-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let target_dir = dir.join("target");
        fs::create_dir_all(&target_dir).unwrap();

        let packages = vec![make_package("app"), make_package("app")];
        let err = stage_binaries_from_target_dir(
            &workspace,
            Arch::Aarch64,
            "aarch64-unknown-linux-gnu",
            &target_dir,
            true,
            &packages,
        )
        .unwrap_err();

        assert!(err.to_string().contains("duplicate binary name"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_fails_when_binary_not_found() {
        let dir = std::env::temp_dir().join(format!("ct-stage-miss-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let target_dir = dir.join("target");
        let binary_dir = target_dir.join("aarch64-unknown-linux-gnu/release");
        fs::create_dir_all(&binary_dir).unwrap();
        // no binary file created

        let packages = vec![make_package("missing-app")];
        let err = stage_binaries_from_target_dir(
            &workspace,
            Arch::Aarch64,
            "aarch64-unknown-linux-gnu",
            &target_dir,
            true,
            &packages,
        )
        .unwrap_err();

        assert!(err.to_string().contains("not found"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_uses_debug_profile_dir() {
        let dir = std::env::temp_dir().join(format!("ct-stage-debug-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let target_dir = dir.join("target");
        let binary_dir = target_dir.join("aarch64-unknown-linux-gnu/debug");
        fs::create_dir_all(&binary_dir).unwrap();
        fs::write(binary_dir.join("dbg-app"), b"debug binary").unwrap();

        let packages = vec![make_package("dbg-app")];
        let result = stage_binaries_from_target_dir(
            &workspace,
            Arch::Aarch64,
            "aarch64-unknown-linux-gnu",
            &target_dir,
            false, // debug
            &packages,
        )
        .unwrap();

        assert_eq!(result.package_names, vec!["dbg-app"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_multiple_binaries() {
        let dir = std::env::temp_dir().join(format!("ct-stage-multi-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let target_dir = dir.join("target");
        let binary_dir = target_dir.join("aarch64-unknown-linux-gnu/release");
        fs::create_dir_all(&binary_dir).unwrap();
        fs::write(binary_dir.join("app-a"), b"binary a").unwrap();
        fs::write(binary_dir.join("app-b"), b"binary b").unwrap();

        let packages = vec![make_package("app-a"), make_package("app-b")];
        let result = stage_binaries_from_target_dir(
            &workspace,
            Arch::Aarch64,
            "aarch64-unknown-linux-gnu",
            &target_dir,
            true,
            &packages,
        )
        .unwrap();

        assert_eq!(result.package_names.len(), 2);
        assert_eq!(result.staged_binaries.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
