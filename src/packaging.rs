use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub const DEFAULT_PACKAGING_DIR: &str = "tizen";
pub const TPK_REFERENCE_PROJECT: &str = "templates/reference-projects/tpk-service-app";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingRpmSpec {
    pub expected_path: PathBuf,
    pub rpm_dir: PathBuf,
    pub alternate_specs: Vec<PathBuf>,
}

impl MissingRpmSpec {
    pub fn command_message(&self, package_name: &str) -> String {
        if self.alternate_specs.is_empty() {
            return format!(
                "RPM spec not found for package `{package_name}`\n\n\
                 no RPM spec files found in: {}\n\
                 run: cargo tizen init --rpm\n\n\
                 expected at: {}",
                self.rpm_dir.display(),
                self.expected_path.display()
            );
        }

        format!(
            "RPM spec not found for package `{package_name}`\n\n\
             expected at: {}\n\n\
             different RPM spec file(s) found in: {}\n\
             {}\n\n\
             if you changed [package].name, rename the spec or regenerate it with:\n\
               cargo tizen init --rpm",
            self.expected_path.display(),
            self.rpm_dir.display(),
            self.render_alternate_specs()
        )
    }

    pub fn doctor_message(&self) -> String {
        if self.alternate_specs.is_empty() {
            return format!(
                "rpm spec missing: {}\n\
                 no RPM spec files found in: {}\n\
                 generate with: cargo tizen init --rpm",
                self.expected_path.display(),
                self.rpm_dir.display()
            );
        }

        format!(
            "rpm spec missing: {}\n\
             different RPM spec file(s) found in: {}\n\
             {}\n\
             if you changed [package].name, rename the spec or regenerate it with: cargo tizen init --rpm",
            self.expected_path.display(),
            self.rpm_dir.display(),
            self.render_alternate_specs()
        )
    }

    fn render_alternate_specs(&self) -> String {
        self.alternate_specs
            .iter()
            .map(|path| {
                format!(
                    "  - {}",
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpmSpecStatus {
    Found(PathBuf),
    Missing(MissingRpmSpec),
}

#[derive(Debug, Clone)]
pub struct PackagingLayout {
    workspace_root: PathBuf,
    root: PathBuf,
}

impl PackagingLayout {
    pub fn new(workspace_root: &Path, custom_root: Option<&Path>) -> Self {
        let root = custom_root
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.join(DEFAULT_PACKAGING_DIR));
        Self {
            workspace_root: workspace_root.to_path_buf(),
            root,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the artifacts staging directory for a given architecture.
    ///
    /// Layout: `<packaging-root>/<arch>/`
    #[allow(dead_code)]
    pub fn artifacts_dir(&self, arch: crate::arch::Arch) -> PathBuf {
        self.root.join(arch.as_str())
    }

    pub fn rpm_spec_path(&self, package_name: &str) -> PathBuf {
        self.rpm_dir().join(format!("{package_name}.spec"))
    }

    pub fn resolve_rpm_spec(&self, package_name: &str) -> Result<PathBuf> {
        match self.inspect_rpm_spec(package_name)? {
            RpmSpecStatus::Found(path) => Ok(path),
            RpmSpecStatus::Missing(missing) => bail!(missing.command_message(package_name)),
        }
    }

    pub fn inspect_rpm_spec(&self, package_name: &str) -> Result<RpmSpecStatus> {
        let path = self.rpm_spec_path(package_name);
        if path.is_file() {
            return Ok(RpmSpecStatus::Found(path));
        }

        let alternate_specs = self.collect_rpm_specs()?;
        Ok(RpmSpecStatus::Missing(MissingRpmSpec {
            expected_path: path,
            rpm_dir: self.rpm_dir(),
            alternate_specs,
        }))
    }

    fn rpm_dir(&self) -> PathBuf {
        self.root.join("rpm")
    }

    fn collect_rpm_specs(&self) -> Result<Vec<PathBuf>> {
        let rpm_dir = self.rpm_dir();
        if !rpm_dir.exists() {
            return Ok(Vec::new());
        }
        if !rpm_dir.is_dir() {
            bail!(
                "RPM packaging directory exists but is not a directory\n\
                 path: {}\n\
                 expected layout: <packaging-dir>/rpm/",
                rpm_dir.display()
            );
        }

        let mut specs = Vec::new();
        for entry in fs::read_dir(&rpm_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("spec") {
                specs.push(path);
            }
        }
        specs.sort();
        Ok(specs)
    }

    pub fn rpm_sources_dir(&self) -> Result<Option<PathBuf>> {
        self.optional_dir(
            self.root.join("rpm").join("sources"),
            "RPM sources directory",
            "optional layout: <packaging-dir>/rpm/sources/",
        )
    }

    pub fn tpk_manifest_path(&self) -> PathBuf {
        self.root.join("tpk").join("tizen-manifest.xml")
    }

    pub fn resolve_tpk_manifest(&self) -> Result<PathBuf> {
        let path = self.tpk_manifest_path();
        if path.is_file() {
            return Ok(path);
        }

        // Check for legacy manifest locations before giving generic advice
        let legacy_root_manifest = self.workspace_root.join("tizen-manifest.xml");
        let legacy_tizen_manifest = self.workspace_root.join("tizen").join("tizen-manifest.xml");
        if legacy_root_manifest.is_file() {
            bail!(
                "TPK manifest not found\n\n\
                 legacy manifest detected at {}\n\
                 move it to: {}",
                legacy_root_manifest.display(),
                path.display()
            )
        }
        if legacy_tizen_manifest.is_file() && legacy_tizen_manifest != path {
            bail!(
                "TPK manifest not found\n\n\
                 legacy manifest detected at {}\n\
                 move it to: {}",
                legacy_tizen_manifest.display(),
                path.display()
            )
        }

        bail!(
            "TPK manifest not found\n\n\
             run: cargo tizen init --tpk\n\n\
             expected at: {}",
            path.display()
        )
    }

    pub fn tpk_reference_dir(&self) -> Result<Option<PathBuf>> {
        self.optional_dir(
            self.root.join("tpk").join("reference"),
            "TPK reference directory",
            "optional layout: <packaging-dir>/tpk/reference",
        )
    }

    pub fn tpk_extra_dir(&self) -> Result<Option<PathBuf>> {
        self.optional_dir(
            self.root.join("tpk").join("extra"),
            "TPK extra directory",
            "optional layout: <packaging-dir>/tpk/extra",
        )
    }

    fn optional_dir(
        &self,
        path: PathBuf,
        label: &str,
        layout_hint: &str,
    ) -> Result<Option<PathBuf>> {
        if !path.exists() {
            return Ok(None);
        }
        if path.is_dir() {
            return Ok(Some(path));
        }

        bail!(
            "{label} exists but is not a directory\npath: {}\n{layout_hint}",
            path.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use super::{PackagingLayout, RpmSpecStatus, TPK_REFERENCE_PROJECT};

    const RPM_REFERENCE_PROJECT: &str = "templates/reference-projects/rpm-app";
    const RPM_SERVICE_REFERENCE_PROJECT: &str = "templates/reference-projects/rpm-service-app";
    const RPM_MULTI_REFERENCE_PROJECT: &str = "templates/reference-projects/rpm-multi-package";

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn rpm_reference_project_uses_standard_layout() {
        let workspace = repo_root().join(RPM_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        let spec = layout
            .resolve_rpm_spec("hello-rpm")
            .expect("rpm reference project should include a standard spec");
        assert_eq!(spec, workspace.join("tizen/rpm/hello-rpm.spec"));
    }

    #[test]
    fn tpk_reference_project_uses_standard_layout() {
        let workspace = repo_root().join(TPK_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        let manifest = layout
            .resolve_tpk_manifest()
            .expect("tpk reference project should include a standard manifest");
        assert_eq!(manifest, workspace.join("tizen/tpk/tizen-manifest.xml"));
    }

    #[test]
    fn custom_packaging_root_is_resolved_explicitly() {
        let workspace = repo_root().join("templates");
        let custom_root = repo_root().join(TPK_REFERENCE_PROJECT).join("tizen");
        let layout = PackagingLayout::new(&workspace, Some(&custom_root));
        let manifest = layout
            .resolve_tpk_manifest()
            .expect("custom packaging root should override the workspace default");
        assert_eq!(
            manifest,
            repo_root()
                .join(TPK_REFERENCE_PROJECT)
                .join("tizen/tpk/tizen-manifest.xml")
        );
    }

    #[test]
    fn rpm_service_reference_project_uses_standard_layout() {
        let workspace = repo_root().join(RPM_SERVICE_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        layout
            .resolve_rpm_spec("hello-service")
            .expect("rpm service reference project should include a standard spec");
    }

    #[test]
    fn rpm_service_reference_project_has_sources_dir() {
        let workspace = repo_root().join(RPM_SERVICE_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        let sources = layout
            .rpm_sources_dir()
            .expect("sources dir should resolve")
            .expect("sources dir should exist");
        assert!(sources.join("hello-service.service").is_file());
        assert!(sources.join("hello-service.env").is_file());
    }

    #[test]
    fn rpm_app_without_sources_dir_returns_none() {
        let workspace = repo_root().join(RPM_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        let sources = layout
            .rpm_sources_dir()
            .expect("absent sources dir should not error");
        assert!(sources.is_none());
    }

    #[test]
    fn rpm_multi_package_resolves_spec_by_name() {
        let workspace = repo_root().join(RPM_MULTI_REFERENCE_PROJECT);
        let layout = PackagingLayout::new(&workspace, None);
        // The multi-package project uses name = "hello-multi" in .cargo-tizen.toml,
        // so the spec file should be hello-multi.spec (not hello-server.spec).
        let spec = layout
            .resolve_rpm_spec("hello-multi")
            .expect("multi-package project should resolve spec by name field");
        assert_eq!(spec, workspace.join("tizen/rpm/hello-multi.spec"));
    }

    #[test]
    fn missing_rpm_spec_error_is_actionable() {
        let dir = tempdir().unwrap();
        let layout = PackagingLayout::new(dir.path(), None);
        let err = layout
            .resolve_rpm_spec("missing-app")
            .expect_err("missing spec should error")
            .to_string();
        assert!(err.contains("RPM spec not found"));
        assert!(err.contains("no RPM spec files found"));
        assert!(err.contains("cargo tizen init --rpm"));
    }

    #[test]
    fn missing_rpm_spec_reports_alternate_specs() {
        let dir = tempdir().unwrap();
        let rpm_dir = dir.path().join("tizen/rpm");
        fs::create_dir_all(&rpm_dir).unwrap();
        fs::write(rpm_dir.join("old-name.spec"), "Name: old-name\n").unwrap();

        let layout = PackagingLayout::new(dir.path(), None);
        let err = layout
            .resolve_rpm_spec("new-name")
            .expect_err("mismatched spec should error")
            .to_string();

        assert!(err.contains("different RPM spec file(s) found"));
        assert!(err.contains("old-name.spec"));
        assert!(err.contains("rename the spec or regenerate it"));
    }

    #[test]
    fn inspect_rpm_spec_reports_alternates_for_doctor() {
        let dir = tempdir().unwrap();
        let rpm_dir = dir.path().join("tizen/rpm");
        fs::create_dir_all(&rpm_dir).unwrap();
        fs::write(rpm_dir.join("legacy.spec"), "Name: legacy\n").unwrap();

        let layout = PackagingLayout::new(dir.path(), None);
        let status = layout.inspect_rpm_spec("current").unwrap();

        match status {
            RpmSpecStatus::Missing(missing) => {
                let message = missing.doctor_message();
                assert!(message.contains("rpm spec missing"));
                assert!(message.contains("different RPM spec file(s) found"));
                assert!(message.contains("legacy.spec"));
            }
            RpmSpecStatus::Found(path) => panic!("expected missing spec, got {}", path.display()),
        }
    }

    #[test]
    fn missing_tpk_manifest_error_is_actionable() {
        let layout = PackagingLayout::new(&repo_root().join("templates"), None);
        let err = layout
            .resolve_tpk_manifest()
            .expect_err("missing manifest should error")
            .to_string();
        assert!(err.contains("TPK manifest not found"));
        assert!(err.contains("cargo tizen init --tpk"));
    }
}
