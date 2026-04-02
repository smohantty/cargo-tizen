use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::InitArgs;
use crate::config::Config;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};
use crate::package_select::{ManifestKind, inspect_manifest, workspace_selection_message};
use crate::packaging::PackagingLayout;

pub fn run_init(ctx: &AppContext, args: &InitArgs) -> Result<()> {
    let targets = selected_targets(args);
    let package = resolve_scaffold_package(ctx, args)?;
    let packaging_root = project_packaging_root(&ctx.workspace_root);
    let packaging = PackagingLayout::new(&ctx.workspace_root, packaging_root.as_deref());

    let mut outcomes = Vec::new();
    outcomes.push(write_scaffold_file(
        &ctx.workspace_root.join(".cargo-tizen.toml"),
        &render_project_config(&package.name, package.pin_default_package),
        false,
    )?);

    if targets.rpm {
        outcomes.push(write_scaffold_file(
            &packaging.rpm_spec_path(&package.name),
            &render_rpm_spec(&package),
            args.force,
        )?);
    }

    if targets.tpk {
        outcomes.push(write_scaffold_file(
            &packaging.tpk_manifest_path(),
            &render_tpk_manifest(&package),
            args.force,
        )?);
    }

    let use_color = color_enabled();
    for outcome in &outcomes {
        let label = match outcome.status {
            ScaffoldStatus::Created => cargo_status(use_color, "Created"),
            ScaffoldStatus::Overwritten => cargo_status(use_color, "Overwrote"),
            ScaffoldStatus::Skipped => cargo_status(use_color, "Skipped"),
        };
        ctx.info(format!("{} {}", label, outcome.path.display()));
    }

    if outcomes
        .iter()
        .all(|outcome| outcome.status == ScaffoldStatus::Skipped)
    {
        ctx.info("no scaffold files were created");
        ctx.info("rerun with --force to overwrite existing scaffold files");
    }

    let arch_hint = ctx.config.default.arch.as_deref().unwrap_or("<arch>");

    ctx.info("");
    ctx.info("Next steps:");
    ctx.info("  edit the generated files to match your app metadata and install paths");
    ctx.info("  run: cargo tizen doctor");
    if !targets.rpm && !targets.tpk {
        ctx.info(
            "  add packaging scaffolds with: cargo tizen init --rpm or cargo tizen init --tpk",
        );
    }
    if targets.rpm {
        ctx.info(format!(
            "  build an RPM with: cargo tizen rpm -A {arch_hint} --release"
        ));
    }
    if targets.tpk {
        ctx.info(format!(
            "  build a TPK with: cargo tizen tpk -A {arch_hint} --release"
        ));
        ctx.info("  optionally set a signing profile with: cargo tizen config --sign <profile>");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct InitTargets {
    rpm: bool,
    tpk: bool,
}

fn selected_targets(args: &InitArgs) -> InitTargets {
    InitTargets {
        rpm: args.rpm,
        tpk: args.tpk,
    }
}

#[derive(Debug, Clone)]
struct ScaffoldPackage {
    name: String,
    version: String,
    license: Option<String>,
    description: Option<String>,
    pin_default_package: bool,
}

fn resolve_scaffold_package(ctx: &AppContext, args: &InitArgs) -> Result<ScaffoldPackage> {
    let manifest_path = ctx.workspace_root.join("Cargo.toml");
    let manifest_kind = inspect_manifest(&manifest_path)?;

    let (package_name, pin_default_package, selection_is_explicit) = if let Some(name) =
        &args.package
    {
        (name.clone(), true, true)
    } else if let Some(name) = ctx.config.default_package() {
        (
            name.to_string(),
            matches!(manifest_kind, ManifestKind::Workspace),
            true,
        )
    } else {
        match &manifest_kind {
            ManifestKind::Package(name) => (name.clone(), false, false),
            ManifestKind::Workspace => bail!(workspace_selection_message(&manifest_path, "init")),
            ManifestKind::Unknown => bail!(
                "failed to determine package name from {}\nexpected a root [package].name or pass -p/--package <member>",
                manifest_path.display()
            ),
        }
    };

    let metadata = load_cargo_metadata(&ctx.workspace_root)?;
    let package = metadata
        .packages
        .iter()
        .find(|pkg| pkg.name == package_name)
        .cloned();

    match package {
        Some(pkg) => Ok(ScaffoldPackage {
            name: pkg.name,
            version: pkg.version,
            license: pkg.license,
            description: pkg.description,
            pin_default_package,
        }),
        None if selection_is_explicit => bail!(
            "package `{package_name}` was not found in cargo metadata\nrerun with a valid workspace member or package name"
        ),
        None => Ok(ScaffoldPackage {
            name: package_name,
            version: "0.1.0".to_string(),
            license: None,
            description: None,
            pin_default_package,
        }),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataPackage {
    name: String,
    version: String,
    license: Option<String>,
    description: Option<String>,
}

fn load_cargo_metadata(workspace_root: &Path) -> Result<CargoMetadata> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(workspace_root)
        .output()
        .context("failed to run cargo metadata")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed with status: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout).context("failed to parse cargo metadata output")
}

fn project_packaging_root(workspace_root: &Path) -> Option<PathBuf> {
    let path = workspace_root.join(".cargo-tizen.toml");
    let raw = fs::read_to_string(path).ok()?;
    let config: Config = toml::from_str(&raw).ok()?;
    config.packaging_dir()
}

fn render_project_config(package_name: &str, pin_default_package: bool) -> String {
    let mut config = String::from(
        "# Generated by cargo tizen init.\n\
         [default]\n\
         arch = \"aarch64\"\n\
         profile = \"mobile\"\n\
         platform_version = \"10.0\"\n",
    );

    if pin_default_package {
        config.push_str(&format!("package = \"{package_name}\"\n"));
    }

    config
}

fn render_rpm_spec(package: &ScaffoldPackage) -> String {
    let summary = package
        .description
        .as_deref()
        .map(single_line)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| format!("cargo-tizen generated RPM package for {}", package.name));
    let license = package
        .license
        .clone()
        .unwrap_or_else(|| "LicenseRef-Unknown".to_string());
    let description = package
        .description
        .as_deref()
        .map(single_line)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| format!("Starter RPM spec for the `{}` binary.", package.name));

    format!(
        "Name:           {name}\n\
         Version:        {version}\n\
         Release:        1%{{?dist}}\n\
         Summary:        {summary}\n\
         License:        {license}\n\
         BuildArch:      %{{_target_cpu}}\n\
         Source0:        {name}\n\
         \n\
         %description\n\
         {description}\n\
         \n\
         %prep\n\
         \n\
         %build\n\
         \n\
         %install\n\
         install -Dm0755 %{{SOURCE0}} %{{buildroot}}/usr/bin/{name}\n\
         \n\
         %files\n\
         /usr/bin/{name}\n",
        name = package.name,
        version = package.version,
        summary = summary,
        license = license,
        description = description,
    )
}

fn render_tpk_manifest(package: &ScaffoldPackage) -> String {
    let app_id = format!("org.example.{}", sanitize_identifier_segment(&package.name));
    let label = title_case_label(&package.name);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <!-- Generated by cargo tizen init. Edit the package, appid, label, and profile before shipping. -->\n\
         <manifest xmlns=\"http://tizen.org/ns/packages\" package=\"{app_id}\" version=\"{version}\" api-version=\"10.0\">\n\
             <profile name=\"mobile\" />\n\
             <service-application appid=\"{app_id}\" exec=\"{exec}\" type=\"capp\" multiple=\"false\" taskmanage=\"false\">\n\
                 <label>{label}</label>\n\
             </service-application>\n\
         </manifest>\n",
        app_id = app_id,
        version = package.version,
        exec = package.name,
        label = label,
    )
}

fn single_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_identifier_segment(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }

    while out.contains("__") {
        out = out.replace("__", "_");
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        return "app".to_string();
    }

    if trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return format!("app_{trimmed}");
    }

    trimmed.to_string()
}

fn title_case_label(raw: &str) -> String {
    let words = raw
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = String::new();
                    out.push(first.to_ascii_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>();

    if words.is_empty() {
        "Tizen App".to_string()
    } else {
        words.join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScaffoldStatus {
    Created,
    Overwritten,
    Skipped,
}

#[derive(Debug, Clone)]
struct ScaffoldOutcome {
    path: PathBuf,
    status: ScaffoldStatus,
}

fn write_scaffold_file(path: &Path, content: &str, force: bool) -> Result<ScaffoldOutcome> {
    if path.exists() && !path.is_file() {
        bail!(
            "scaffold target exists but is not a file: {}",
            path.display()
        );
    }

    let existed = path.exists();
    if existed && !force {
        return Ok(ScaffoldOutcome {
            path: path.to_path_buf(),
            status: ScaffoldStatus::Skipped,
        });
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;

    Ok(ScaffoldOutcome {
        path: path.to_path_buf(),
        status: if existed {
            ScaffoldStatus::Overwritten
        } else {
            ScaffoldStatus::Created
        },
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        ScaffoldPackage, ScaffoldStatus, render_project_config, render_rpm_spec,
        render_tpk_manifest, selected_targets, write_scaffold_file,
    };
    use crate::cli::InitArgs;

    fn init_args() -> InitArgs {
        InitArgs {
            rpm: false,
            tpk: false,
            package: None,
            force: false,
        }
    }

    #[test]
    fn default_init_targets_do_not_create_packaging_scaffolds() {
        let targets = selected_targets(&init_args());
        assert_eq!((targets.rpm, targets.tpk), (false, false));
    }

    #[test]
    fn project_config_pins_selected_package_when_requested() {
        let config = render_project_config("demo-app", true);
        assert!(config.contains("package = \"demo-app\""));
        assert!(config.contains("[default]"));
        assert!(config.contains("arch = \"aarch64\""));
        assert!(config.contains("profile = \"mobile\""));
        assert!(config.contains("platform_version = \"10.0\""));
    }

    #[test]
    fn project_config_is_created_even_when_not_needed_for_package_selection() {
        let config = render_project_config("demo-app", false);
        assert!(config.contains("arch = \"aarch64\""));
        assert!(config.contains("profile = \"mobile\""));
        assert!(config.contains("platform_version = \"10.0\""));
        assert!(!config.contains("package ="));
    }

    #[test]
    fn rpm_spec_uses_target_cpu_macro() {
        let package = ScaffoldPackage {
            name: "demo-app".to_string(),
            version: "1.2.3".to_string(),
            license: Some("MIT".to_string()),
            description: Some("Demo package".to_string()),
            pin_default_package: false,
        };
        let spec = render_rpm_spec(&package);
        assert!(spec.contains("BuildArch:      %{_target_cpu}"));
        assert!(spec.contains("Version:        1.2.3"));
        assert!(spec.contains("Source0:        demo-app"));
    }

    #[test]
    fn tpk_manifest_uses_package_name_as_exec() {
        let package = ScaffoldPackage {
            name: "demo-app".to_string(),
            version: "0.1.0".to_string(),
            license: None,
            description: None,
            pin_default_package: false,
        };
        let manifest = render_tpk_manifest(&package);
        assert!(manifest.contains("exec=\"demo-app\""));
        assert!(manifest.contains("package=\"org.example.demo_app\""));
    }

    #[test]
    fn write_scaffold_file_skips_existing_without_force() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("demo.txt");
        fs::write(&path, "before").unwrap();

        let outcome = write_scaffold_file(&path, "after", false).unwrap();
        assert_eq!(outcome.status, ScaffoldStatus::Skipped);
        assert_eq!(fs::read_to_string(&path).unwrap(), "before");
    }

    #[test]
    fn write_scaffold_file_overwrites_with_force() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("demo.txt");
        fs::write(&path, "before").unwrap();

        let outcome = write_scaffold_file(&path, "after", true).unwrap();
        assert_eq!(outcome.status, ScaffoldStatus::Overwritten);
        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
    }

    #[test]
    fn write_scaffold_file_without_force_never_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".cargo-tizen.toml");
        fs::write(&path, "before").unwrap();

        let outcome = write_scaffold_file(&path, "after", false).unwrap();
        assert_eq!(outcome.status, ScaffoldStatus::Skipped);
        assert_eq!(fs::read_to_string(&path).unwrap(), "before");
    }
}
