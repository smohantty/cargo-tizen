use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::arch::Arch;

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct StageOutput {
    pub stage_root: PathBuf,
    pub staged_binary: PathBuf,
    pub package: PackageInfo,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: ManifestPackage,
}

#[derive(Debug, Deserialize)]
struct ManifestPackage {
    name: String,
}

pub fn stage_binary_from_target_dir(
    workspace_root: &Path,
    arch: Arch,
    rust_target: &str,
    target_dir: &Path,
    release: bool,
) -> Result<StageOutput> {
    let package = read_manifest_package(&workspace_root.join("Cargo.toml"))?;
    let profile_dir = if release { "release" } else { "debug" };
    let source_binary = target_dir
        .join(rust_target)
        .join(profile_dir)
        .join(&package.name);

    if !source_binary.exists() {
        bail!(
            "expected built binary was not found: {}",
            source_binary.display()
        );
    }

    let stage_root = workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join(profile_dir)
        .join("stage");
    let staged_binary = stage_root.join("usr/bin").join(&package.name);

    fs::create_dir_all(staged_binary.parent().unwrap_or(&stage_root)).with_context(|| {
        format!(
            "failed to create staging directory for {}",
            staged_binary.display()
        )
    })?;
    fs::copy(&source_binary, &staged_binary).with_context(|| {
        format!(
            "failed to copy built binary {} -> {}",
            source_binary.display(),
            staged_binary.display()
        )
    })?;

    Ok(StageOutput {
        stage_root,
        staged_binary,
        package,
    })
}

fn read_manifest_package(path: &Path) -> Result<PackageInfo> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    let parsed: CargoManifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse manifest {}", path.display()))?;
    Ok(PackageInfo {
        name: parsed.package.name,
    })
}
