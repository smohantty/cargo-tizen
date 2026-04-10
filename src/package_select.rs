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
    workspace: Option<serde_json::Value>,
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

    if let Some(packages) = configured_packages(ctx)? {
        return Ok(packages[0].clone());
    }

    let manifest_path = ctx.workspace_root.join("Cargo.toml");
    match inspect_manifest(&manifest_path)? {
        ManifestKind::Package(name) => Ok(SelectedPackage {
            name,
            source: PackageSource::Manifest,
        }),
        ManifestKind::Workspace => bail!(workspace_selection_message(&manifest_path, command_name)),
        ManifestKind::Unknown => bail!(
            "failed to determine package name from {}\nexpected a root [package].name, pass -p/--package <member>, or set [package].packages in .cargo-tizen.toml",
            manifest_path.display()
        ),
    }
}

pub fn inspect_manifest(path: &Path) -> Result<ManifestKind> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Cargo manifest {}", path.display()))?;
    let parsed: CargoManifest = basic_toml::from_str(&raw)
        .with_context(|| format!("failed to parse Cargo manifest {}", path.display()))?;

    if let Some(package) = parsed.package {
        return Ok(ManifestKind::Package(package.name));
    }

    if parsed.workspace.is_some() {
        return Ok(ManifestKind::Workspace);
    }

    Ok(ManifestKind::Unknown)
}

/// Resolve packages for RPM packaging. Returns one or more packages to build and stage.
///
/// Resolution priority:
/// 1. CLI `-p <name>` → single package override
/// 2. `[package].packages` from config → one or more packages
/// 3. Root `[package].name` from Cargo.toml → single package auto-detect
/// 4. Workspace root with no selection → error
pub fn resolve_rpm_packages(
    ctx: &AppContext,
    explicit_package: Option<&str>,
) -> Result<Vec<SelectedPackage>> {
    // 1. CLI -p override: always single package
    if let Some(name) = explicit_package {
        return Ok(vec![SelectedPackage {
            name: name.to_string(),
            source: PackageSource::Cli,
        }]);
    }

    // 2. [package].packages from config
    if let Some(packages) = configured_packages(ctx)? {
        return Ok(packages);
    }

    // 3-4. Fall through to existing single-package resolution
    resolve_for_command(ctx, None, "rpm").map(|pkg| vec![pkg])
}

pub fn resolve_configured_packages(ctx: &AppContext) -> Result<Option<Vec<SelectedPackage>>> {
    configured_packages(ctx)
}

pub fn workspace_selection_message(path: &Path, command_name: &str) -> String {
    format!(
        "workspace root detected at {}\n`cargo tizen {}` needs a package selection\nrerun with: cargo tizen {} -p <member>\nor set packages in .cargo-tizen.toml:\n[package]\npackages = [\"<member>\"]\nor for multi-binary RPM:\n[package]\npackages = [\"<member-a>\", \"<member-b>\"]",
        path.display(),
        command_name,
        command_name
    )
}

fn configured_packages(ctx: &AppContext) -> Result<Option<Vec<SelectedPackage>>> {
    let Some(packages) = ctx.config.package_names() else {
        return Ok(None);
    };

    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(packages.len());
    for name in packages {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!(
                "empty package name in [package].packages\n\
                 each entry must be a non-empty Cargo package name"
            );
        }
        if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
            bail!(
                "invalid package name `{name}` in [package].packages\n\
                 package names must not contain path separators or `..`"
            );
        }
        if !seen.insert(name.as_str()) {
            bail!(
                "duplicate package `{name}` in [package].packages\n\
                 each entry must be unique"
            );
        }
        result.push(SelectedPackage {
            name: name.clone(),
            source: PackageSource::Config,
        });
    }

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{
        ManifestKind, PackageSource, inspect_manifest, resolve_for_command, resolve_rpm_packages,
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
        assert!(message.contains("[package]"));
        assert!(message.contains("packages = [\"<member>\"]"));
    }

    #[test]
    fn resolve_for_command_uses_first_configured_package() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.package.packages = Some(vec!["rsdbd".to_string(), "extra".to_string()]);
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let selected = resolve_for_command(&ctx, None, "rpm").unwrap();
        assert_eq!(selected.name, "rsdbd");
        assert_eq!(selected.source, PackageSource::Config);
    }

    #[test]
    fn resolve_rpm_packages_cli_override_returns_single() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.package.packages = Some(vec!["a".into(), "b".into()]);
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let result = resolve_rpm_packages(&ctx, Some("override")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "override");
        assert_eq!(result[0].source, PackageSource::Cli);
    }

    #[test]
    fn resolve_rpm_packages_from_config() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.package.packages = Some(vec!["a".into(), "b".into()]);
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let result = resolve_rpm_packages(&ctx, None).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "a");
        assert_eq!(result[0].source, PackageSource::Config);
        assert_eq!(result[1].name, "b");
    }

    #[test]
    fn resolve_rpm_packages_empty_config_falls_through() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.package.packages = Some(vec![]); // empty = treated as unset
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let result = resolve_rpm_packages(&ctx, None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "solo");
        assert_eq!(result[0].source, PackageSource::Manifest);
    }

    #[test]
    fn resolve_rpm_packages_rejects_duplicates() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n",
        )
        .unwrap();

        let mut config = Config::default();
        config.package.packages = Some(vec!["same".into(), "same".into()]);
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let err = resolve_rpm_packages(&ctx, None).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn resolve_rpm_packages_fallback_to_manifest() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"standalone\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let ctx = AppContext {
            config: Config::default(),
            workspace_root: dir.path().to_path_buf(),
        };

        let result = resolve_rpm_packages(&ctx, None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "standalone");
        assert_eq!(result[0].source, PackageSource::Manifest);
    }
}
