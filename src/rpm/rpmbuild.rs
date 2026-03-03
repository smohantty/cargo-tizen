use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::context::AppContext;
use crate::tool_env;

pub fn build_rpm(
    ctx: &AppContext,
    workspace_root: &Path,
    rpm_arch: &str,
    arch: Arch,
    profile_dir: &str,
    spec_path: &Path,
    staged_binary: &Path,
    binary_name: &str,
    output_override: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let topdir = workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join(profile_dir)
        .join("rpmbuild");

    for dir in ["BUILD", "RPMS", "SOURCES", "SPECS", "SRPMS", "BUILDROOT"] {
        fs::create_dir_all(topdir.join(dir))
            .with_context(|| format!("failed to create rpmbuild directory {}", dir))?;
    }

    let source_binary = topdir.join("SOURCES").join(binary_name);
    fs::copy(staged_binary, &source_binary).with_context(|| {
        format!(
            "failed to copy staged binary {} -> {}",
            staged_binary.display(),
            source_binary.display()
        )
    })?;

    let spec_in_topdir = topdir.join("SPECS").join(
        spec_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid spec path: {}", spec_path.display()))?,
    );
    if spec_path != spec_in_topdir {
        fs::copy(spec_path, &spec_in_topdir).with_context(|| {
            format!(
                "failed to copy spec file {} -> {}",
                spec_path.display(),
                spec_in_topdir.display()
            )
        })?;
    }

    let mut command = Command::new("rpmbuild");
    command
        .arg("-bb")
        .arg(&spec_in_topdir)
        .arg("--target")
        .arg(rpm_arch)
        .arg("--define")
        .arg(format!("_topdir {}", topdir.display()));

    if let Some(out) = output_override {
        fs::create_dir_all(out)
            .with_context(|| format!("failed to create output directory {}", out.display()))?;
        command
            .arg("--define")
            .arg(format!("_rpmdir {}", out.display()));
    }

    tool_env::rpmbuild_env(ctx).apply(&mut command);

    let status = command.status().context("failed to execute rpmbuild")?;
    if !status.success() {
        bail!("rpmbuild failed with status: {status}");
    }

    let rpm_root = output_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| topdir.join("RPMS"));

    collect_rpms(&rpm_root)
}

fn collect_rpms(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to list RPM output directory {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_rpms(&path)?);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rpm") {
            files.push(path);
        }
    }

    Ok(files)
}
