use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::context::AppContext;
use crate::tool_env;

pub fn collect_extra_sources(sources_dir: &Path, binary_names: &[&str]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut seen_names = HashSet::new();
    for name in binary_names {
        seen_names.insert(name.to_string());
    }

    let mut entries: Vec<_> = fs::read_dir(sources_dir)
        .with_context(|| format!("failed to read RPM sources dir {}", sources_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to iterate RPM sources dir {}",
                sources_dir.display()
            )
        })?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if meta.is_symlink() {
            bail!(
                "RPM sources dir contains symlink: {}\nsymlinks are not supported; use regular files only",
                path.display()
            );
        }

        if !meta.is_file() {
            continue;
        }

        if !seen_names.insert(name.clone()) {
            bail!(
                "RPM source name collision: `{name}` conflicts with a staged binary or another source\n\
                 all files in <packaging-dir>/rpm/sources/ must have names distinct from\n\
                 all staged binaries: [{}]",
                binary_names.join(", ")
            );
        }

        files.push(path);
    }

    Ok(files)
}

pub fn build_rpm(
    ctx: &AppContext,
    workspace_root: &Path,
    rpm_arch: &str,
    arch: Arch,
    profile_dir: &str,
    spec_path: &Path,
    staged_binaries: &[PathBuf],
    extra_sources: &[PathBuf],
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

    // Clean SOURCES dir to remove stale files from previous runs
    let sources_dir = topdir.join("SOURCES");
    for entry in fs::read_dir(&sources_dir)
        .with_context(|| format!("failed to read SOURCES dir {}", sources_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).with_context(|| {
                format!("failed to clean stale SOURCES entry {}", path.display())
            })?;
        } else {
            fs::remove_file(&path).with_context(|| {
                format!("failed to clean stale SOURCES file {}", path.display())
            })?;
        }
    }

    for staged in staged_binaries {
        let binary_name = staged
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid staged binary path: {}", staged.display()))?;
        let dest = sources_dir.join(binary_name);
        fs::copy(staged, &dest).with_context(|| {
            format!(
                "failed to copy staged binary {} -> {}",
                staged.display(),
                dest.display()
            )
        })?;
    }

    for source in extra_sources {
        let dest = topdir.join("SOURCES").join(
            source
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("invalid source path: {}", source.display()))?,
        );
        fs::copy(source, &dest).with_context(|| {
            format!(
                "failed to copy RPM source {} -> {}",
                source.display(),
                dest.display()
            )
        })?;
    }

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

    // Host rpmbuild brp strip helpers use host binutils by default.
    // For cross-arch RPMs this fails on target binaries (e.g. ARM on x86_64),
    // so disable strip post-processing hooks automatically.
    if is_cross_rpm_build(arch) {
        ctx.debug(format!(
            "cross-arch RPM build detected (host={} target={}); disabling brp strip hooks",
            std::env::consts::ARCH,
            arch.as_str()
        ));
        command
            .arg("--define")
            .arg("__brp_strip /bin/true")
            .arg("--define")
            .arg("__brp_strip_static_archive /bin/true")
            .arg("--define")
            .arg("__brp_strip_comment_note /bin/true");

        // rpmbuild refuses to build for a foreign architecture unless the
        // host arch is listed as compatible via buildarch_compat.  Generate a
        // small rpmrc override inside the build tree and chain it after the
        // system default so the target arch is accepted.
        let host_rpm_arch = host_rpm_arch();
        let rpmrc_path = topdir.join("cross.rpmrc");
        fs::write(
            &rpmrc_path,
            format!("buildarch_compat: {}: {}\n", host_rpm_arch, rpm_arch),
        )
        .with_context(|| {
            format!(
                "failed to write cross-build rpmrc at {}",
                rpmrc_path.display()
            )
        })?;
        ctx.debug(format!(
            "using cross-build rpmrc: {} (buildarch_compat: {}: {})",
            rpmrc_path.display(),
            host_rpm_arch,
            rpm_arch
        ));
        command
            .arg("--rcfile")
            .arg(format!("/usr/lib/rpm/rpmrc:{}", rpmrc_path.display()));
    }

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

fn is_cross_rpm_build(target_arch: Arch) -> bool {
    match Arch::parse(std::env::consts::ARCH) {
        Some(host_arch) => host_arch != target_arch,
        None => true,
    }
}

/// Map the Rust `std::env::consts::ARCH` value to the RPM architecture name
/// used in rpmrc `buildarch_compat` entries.
fn host_rpm_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" | "i686" | "i586" | "i386" => "i686",
        "aarch64" | "arm64" => "aarch64",
        "arm" | "armv7l" | "armv7" => "armv7l",
        other => other, // pass through as-is for unknown hosts
    }
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{collect_extra_sources, is_cross_rpm_build};
    use crate::arch::Arch;

    #[test]
    fn cross_rpm_detection_is_conservative() {
        // Should always be true for a known-nonmatching target.
        let expected_for_armv7l = !matches!(std::env::consts::ARCH, "armv7l" | "armv7" | "arm");
        assert_eq!(is_cross_rpm_build(Arch::Armv7l), expected_for_armv7l);

        let expected_for_aarch64 = !matches!(std::env::consts::ARCH, "aarch64" | "arm64");
        assert_eq!(is_cross_rpm_build(Arch::Aarch64), expected_for_aarch64);
    }

    #[test]
    fn collect_extra_sources_from_reference_project() {
        let sources_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("templates/reference-projects/rpm-service-app/tizen/rpm/sources");
        let files = collect_extra_sources(&sources_dir, &["hello-service"])
            .expect("should collect sources");
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"hello-service.env".to_string()));
        assert!(names.contains(&"hello-service.service".to_string()));
    }

    #[test]
    fn collect_extra_sources_rejects_binary_name_collision() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mybinary"), "fake").unwrap();
        let err =
            collect_extra_sources(dir.path(), &["mybinary"]).expect_err("should reject collision");
        assert!(err.to_string().contains("collision"));
    }

    #[test]
    fn collect_extra_sources_skips_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitkeep"), "").unwrap();
        fs::write(dir.path().join("real-source"), "content").unwrap();
        let files =
            collect_extra_sources(dir.path(), &["mybinary"]).expect("should collect sources");
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().unwrap().to_string_lossy(),
            "real-source"
        );
    }

    #[test]
    fn collect_extra_sources_empty_dir_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let files =
            collect_extra_sources(dir.path(), &["mybinary"]).expect("empty dir should succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn collect_extra_sources_multi_binary_collision() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("server-bin"), "fake").unwrap();
        let err = collect_extra_sources(dir.path(), &["server-bin", "cli-bin"])
            .expect_err("should reject collision with binary name");
        let msg = err.to_string();
        assert!(msg.contains("collision"));
        assert!(msg.contains("server-bin"));
        assert!(msg.contains("cli-bin"));
    }

    #[test]
    fn collect_extra_sources_multi_binary_no_collision() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.ini"), "content").unwrap();
        let files = collect_extra_sources(dir.path(), &["server", "cli"])
            .expect("should succeed without collision");
        assert_eq!(files.len(), 1);
    }
}
