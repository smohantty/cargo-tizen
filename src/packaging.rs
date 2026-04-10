use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

pub const DEFAULT_PACKAGING_DIR: &str = "tizen";
pub const TPK_REFERENCE_PROJECT: &str = "templates/reference-projects/tpk-service-app";

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
        self.root.join("rpm").join(format!("{package_name}.spec"))
    }

    pub fn resolve_rpm_spec(&self, package_name: &str) -> Result<PathBuf> {
        let path = self.rpm_spec_path(package_name);
        if path.is_file() {
            return Ok(path);
        }

        bail!(
            "RPM spec not found for package `{package_name}`\n\n\
             run: cargo tizen init --rpm\n\n\
             expected at: {}",
            path.display()
        )
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
    use super::{PackagingLayout, TPK_REFERENCE_PROJECT};
    use std::path::PathBuf;

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
        let layout = PackagingLayout::new(&repo_root().join("templates"), None);
        let err = layout
            .resolve_rpm_spec("missing-app")
            .expect_err("missing spec should error")
            .to_string();
        assert!(err.contains("RPM spec not found"));
        assert!(err.contains("cargo tizen init --rpm"));
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
