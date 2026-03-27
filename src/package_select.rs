use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::context::AppContext;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedPackage {
    pub name: String,
    pub source: PackageSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSource {
    Cli,
    Config,
    Manifest,
}

impl PackageSource {
    pub fn requires_cargo_package_arg(self) -> bool {
        matches!(self, Self::Cli | Self::Config)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestKind {
    Package(String),
    Workspace,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: Option<ManifestPackage>,
    workspace: Option<toml::Value>,
}

#[derive(Debug, Deserialize)]
struct ManifestPackage {
    name: String,
}

pub fn resolve_for_command(
    ctx: &AppContext,
    explicit_package: Option<&str>,
    command_name: &str,
) -> Result<SelectedPackage> {
    if let Some(name) = explicit_package {
        return Ok(SelectedPackage {
            name: name.to_string(),
            source: PackageSource::Cli,
        });
    }

    if let Some(name) = ctx.config.default_package() {
        return Ok(SelectedPackage {
            name: name.to_string(),
            source: PackageSource::Config,
        });
    }

    let manifest_path = ctx.workspace_root.join("Cargo.toml");
    match inspect_manifest(&manifest_path)? {
        ManifestKind::Package(name) => Ok(SelectedPackage {
            name,
            source: PackageSource::Manifest,
        }),
        ManifestKind::Workspace => bail!(workspace_selection_message(&manifest_path, command_name)),
        ManifestKind::Unknown => bail!(
            "failed to determine package name from {}\nexpected a root [package].name, pass -p/--package <member>, or set [default].package in .cargo-tizen.toml",
            manifest_path.display()
        ),
    }
}

pub fn inspect_manifest(path: &Path) -> Result<ManifestKind> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Cargo manifest {}", path.display()))?;
    let parsed: CargoManifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse Cargo manifest {}", path.display()))?;

    if let Some(package) = parsed.package {
        return Ok(ManifestKind::Package(package.name));
    }

    if parsed.workspace.is_some() {
        return Ok(ManifestKind::Workspace);
    }

    Ok(ManifestKind::Unknown)
}

pub fn workspace_selection_message(path: &Path, command_name: &str) -> String {
    format!(
        "workspace root detected at {}\n`cargo tizen {}` packages one workspace member at a time\nrerun with: cargo tizen {} -p <member>\nor set a default package in .cargo-tizen.toml:\n[default]\npackage = \"<member>\"",
        path.display(),
        command_name,
        command_name
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{
        ManifestKind, PackageSource, inspect_manifest, resolve_for_command,
        workspace_selection_message,
    };
    use crate::config::Config;
    use crate::context::AppContext;

    #[test]
    fn inspect_manifest_reads_root_package_name() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"hello\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        assert_eq!(
            inspect_manifest(&dir.path().join("Cargo.toml")).unwrap(),
            ManifestKind::Package("hello".to_string())
        );
    }

    #[test]
    fn inspect_manifest_detects_workspace_root() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\n",
        )
        .unwrap();

        assert_eq!(
            inspect_manifest(&dir.path().join("Cargo.toml")).unwrap(),
            ManifestKind::Workspace
        );
    }

    #[test]
    fn workspace_message_mentions_flag_and_config() {
        let message = workspace_selection_message(Path::new("/tmp/demo/Cargo.toml"), "rpm");
        assert!(message.contains("workspace root detected"));
        assert!(message.contains("cargo tizen rpm -p <member>"));
        assert!(message.contains("[default]"));
        assert!(message.contains("package = \"<member>\""));
    }

    #[test]
    fn resolve_for_command_uses_default_package_from_config() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.default.package = Some("rsdbd".to_string());
        let ctx = AppContext {
            config,
            verbose: false,
            quiet: true,
            workspace_root: dir.path().to_path_buf(),
        };

        let selected = resolve_for_command(&ctx, None, "rpm").unwrap();
        assert_eq!(selected.name, "rsdbd");
        assert_eq!(selected.source, PackageSource::Config);
    }
}
