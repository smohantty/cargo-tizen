use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::InitArgs;
use crate::config::Config;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};
use crate::package_select;
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
        &render_project_config(&package.name),
        false,
    )?);

    if targets.rpm {
        let rpm_scaffold = resolve_rpm_scaffold(ctx, args, &package)?;
        outcomes.push(write_scaffold_file(
            &packaging.rpm_spec_path(&rpm_scaffold.rpm_name),
            &render_rpm_spec(&rpm_scaffold),
            args.force,
        )?);
        let sources_gitkeep = packaging
            .root()
            .join("rpm")
            .join("sources")
            .join(".gitkeep");
        if !sources_gitkeep.exists() {
            if let Some(parent) = sources_gitkeep.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&sources_gitkeep, "")?;
            outcomes.push(ScaffoldOutcome {
                path: sources_gitkeep,
                status: ScaffoldStatus::Created,
            });
        }
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
}

#[derive(Debug, Clone)]
struct RpmScaffold {
    rpm_name: String,
    package_names: Vec<String>,
    version: String,
    license: Option<String>,
    description: Option<String>,
}

fn resolve_scaffold_package(ctx: &AppContext, args: &InitArgs) -> Result<ScaffoldPackage> {
    let manifest_path = ctx.workspace_root.join("Cargo.toml");
    let manifest_kind = inspect_manifest(&manifest_path)?;
    let metadata = load_cargo_metadata(&ctx.workspace_root)?;
    let (package_name, selection_is_explicit) = if let Some(name) = &args.package {
        (name.clone(), true)
    } else if let Some(name) = ctx.config.primary_package() {
        (name.to_string(), true)
    } else {
        match &manifest_kind {
            ManifestKind::Package(name) => (name.clone(), false),
            ManifestKind::Workspace => {
                let package = select_workspace_package(&metadata).ok_or_else(|| {
                    anyhow::anyhow!(workspace_selection_message(&manifest_path, "init"))
                })?;
                (package.name.clone(), false)
            }
            ManifestKind::Unknown => bail!(
                "failed to determine package name from {}\nexpected a root [package].name or pass -p/--package <member>",
                manifest_path.display()
            ),
        }
    };

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
        }),
        None if selection_is_explicit => bail!(
            "package `{package_name}` was not found in cargo metadata\nrerun with a valid workspace member or package name"
        ),
        None => Ok(ScaffoldPackage {
            name: package_name,
            version: "0.1.0".to_string(),
            license: None,
            description: None,
        }),
    }
}

fn resolve_rpm_scaffold(
    ctx: &AppContext,
    args: &InitArgs,
    package: &ScaffoldPackage,
) -> Result<RpmScaffold> {
    let rpm_name = resolve_rpm_scaffold_name(ctx, package)?;
    let package_names = package_select::resolve_rpm_packages(ctx, args.package.as_deref())?
        .into_iter()
        .map(|pkg| pkg.name)
        .collect();

    Ok(RpmScaffold {
        rpm_name,
        package_names,
        version: package.version.clone(),
        license: package.license.clone(),
        description: package.description.clone(),
    })
}

fn resolve_rpm_scaffold_name(ctx: &AppContext, package: &ScaffoldPackage) -> Result<String> {
    if ctx.workspace_root.join(".cargo-tizen.toml").exists() {
        return ctx
            .config
            .package
            .name()
            .map(ToString::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "existing .cargo-tizen.toml is missing [package].name\n\
                 set [package].name and rerun cargo tizen init --rpm"
                )
            });
    }

    Ok(package.name.clone())
}

#[derive(Debug, Clone, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
    workspace_default_members: Vec<String>,
    workspace_members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    description: Option<String>,
    targets: Vec<MetadataTarget>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataTarget {
    kind: Vec<String>,
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
    let config: Config = basic_toml::from_str(&raw).ok()?;
    config.packaging_dir()
}

fn render_project_config(package_name: &str) -> String {
    format!(
        "# Generated by cargo tizen init.\n\
         [default]\n\
         arch = \"aarch64\"\n\
         profile = \"mobile\"\n\
         platform_version = \"10.0\"\n\
         \n\
         [package]\n\
         name = \"{package_name}\"\n\
         packages = [\"{package_name}\"]\n",
    )
}

fn select_workspace_package(metadata: &CargoMetadata) -> Option<&MetadataPackage> {
    for member_ids in [
        &metadata.workspace_default_members,
        &metadata.workspace_members,
    ] {
        for member_id in member_ids {
            if let Some(package) = metadata
                .packages
                .iter()
                .find(|pkg| pkg.id == *member_id && package_has_bin_target(pkg))
            {
                return Some(package);
            }
        }
        for member_id in member_ids {
            if let Some(package) = metadata.packages.iter().find(|pkg| pkg.id == *member_id) {
                return Some(package);
            }
        }
    }

    metadata
        .packages
        .iter()
        .find(|pkg| package_has_bin_target(pkg))
        .or_else(|| metadata.packages.first())
}

fn package_has_bin_target(package: &MetadataPackage) -> bool {
    package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "bin"))
}

fn render_rpm_spec(scaffold: &RpmScaffold) -> String {
    let summary = scaffold
        .description
        .as_deref()
        .map(single_line)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| {
            format!(
                "cargo-tizen generated RPM package for {}",
                scaffold.rpm_name
            )
        });
    let license = scaffold
        .license
        .clone()
        .unwrap_or_else(|| "LicenseRef-Unknown".to_string());
    let description = scaffold
        .description
        .as_deref()
        .map(single_line)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| match scaffold.package_names.as_slice() {
            [package_name] => format!("Starter RPM spec for the `{package_name}` binary."),
            package_names => format!(
                "Starter RPM spec for the `{}` package. Includes staged binaries: {}.",
                scaffold.rpm_name,
                package_names.join(", ")
            ),
        });
    let source_lines = scaffold
        .package_names
        .iter()
        .enumerate()
        .map(|(index, package_name)| format!("Source{index}:        {package_name}"))
        .collect::<Vec<_>>()
        .join("\n");
    let install_lines = scaffold
        .package_names
        .iter()
        .enumerate()
        .map(|(index, package_name)| {
            format!("install -Dm0755 %{{SOURCE{index}}} %{{buildroot}}/usr/bin/{package_name}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let file_lines = scaffold
        .package_names
        .iter()
        .map(|package_name| format!("/usr/bin/{package_name}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Name:           {name}\n\
         Version:        {version}\n\
         Release:        1%{{?dist}}\n\
         Summary:        {summary}\n\
         License:        {license}\n\
         BuildArch:      %{{_target_cpu}}\n\
         {source_lines}\n\
         \n\
         %description\n\
         {description}\n\
         \n\
         %prep\n\
         \n\
         %build\n\
         \n\
         %install\n\
         {install_lines}\n\
         \n\
         %files\n\
         {file_lines}\n",
        name = scaffold.rpm_name,
        version = scaffold.version,
        summary = summary,
        license = license,
        description = description,
        source_lines = source_lines,
        install_lines = install_lines,
        file_lines = file_lines,
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
        CargoMetadata, MetadataPackage, MetadataTarget, RpmScaffold, ScaffoldPackage,
        ScaffoldStatus, render_project_config, render_rpm_spec, render_tpk_manifest, run_init,
        select_workspace_package, selected_targets, write_scaffold_file,
    };
    use crate::cli::InitArgs;
    use crate::config::Config;
    use crate::context::AppContext;

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
    fn project_config_always_writes_package_list() {
        let config = render_project_config("demo-app");
        assert!(config.contains("[package]"));
        assert!(config.contains("name = \"demo-app\""));
        assert!(config.contains("packages = [\"demo-app\"]"));
        assert!(!config.contains("[release]"));
        assert!(config.contains("[default]"));
        assert!(config.contains("arch = \"aarch64\""));
        assert!(config.contains("profile = \"mobile\""));
        assert!(config.contains("platform_version = \"10.0\""));
    }

    #[test]
    fn select_workspace_package_prefers_first_binary_default_member() {
        let metadata = CargoMetadata {
            packages: vec![
                MetadataPackage {
                    id: "pkg-lib".to_string(),
                    name: "hello-lib".to_string(),
                    version: "0.1.0".to_string(),
                    license: None,
                    description: None,
                    targets: vec![MetadataTarget {
                        kind: vec!["lib".to_string()],
                    }],
                },
                MetadataPackage {
                    id: "pkg-server".to_string(),
                    name: "hello-server".to_string(),
                    version: "0.1.0".to_string(),
                    license: None,
                    description: None,
                    targets: vec![MetadataTarget {
                        kind: vec!["bin".to_string()],
                    }],
                },
                MetadataPackage {
                    id: "pkg-cli".to_string(),
                    name: "hello-cli".to_string(),
                    version: "0.1.0".to_string(),
                    license: None,
                    description: None,
                    targets: vec![MetadataTarget {
                        kind: vec!["bin".to_string()],
                    }],
                },
            ],
            workspace_default_members: vec!["pkg-lib".to_string(), "pkg-server".to_string()],
            workspace_members: vec![
                "pkg-lib".to_string(),
                "pkg-server".to_string(),
                "pkg-cli".to_string(),
            ],
        };

        let package = select_workspace_package(&metadata).unwrap();
        assert_eq!(package.name, "hello-server");
    }

    #[test]
    fn rpm_spec_uses_target_cpu_macro() {
        let scaffold = RpmScaffold {
            rpm_name: "demo-app".to_string(),
            package_names: vec!["demo-app".to_string()],
            version: "1.2.3".to_string(),
            license: Some("MIT".to_string()),
            description: Some("Demo package".to_string()),
        };
        let spec = render_rpm_spec(&scaffold);
        assert!(spec.contains("BuildArch:      %{_target_cpu}"));
        assert!(spec.contains("Version:        1.2.3"));
        assert!(spec.contains("Source0:        demo-app"));
    }

    #[test]
    fn rpm_spec_uses_configured_name_and_all_sources_for_multi_package() {
        let scaffold = RpmScaffold {
            rpm_name: "demo-bundle".to_string(),
            package_names: vec!["demo-server".to_string(), "demo-cli".to_string()],
            version: "1.2.3".to_string(),
            license: Some("MIT".to_string()),
            description: None,
        };

        let spec = render_rpm_spec(&scaffold);
        assert!(spec.contains("Name:           demo-bundle"));
        assert!(spec.contains("Source0:        demo-server"));
        assert!(spec.contains("Source1:        demo-cli"));
        assert!(spec.contains("install -Dm0755 %{SOURCE0} %{buildroot}/usr/bin/demo-server"));
        assert!(spec.contains("install -Dm0755 %{SOURCE1} %{buildroot}/usr/bin/demo-cli"));
        assert!(spec.contains("/usr/bin/demo-server"));
        assert!(spec.contains("/usr/bin/demo-cli"));
    }

    #[test]
    fn tpk_manifest_uses_package_name_as_exec() {
        let package = ScaffoldPackage {
            name: "demo-app".to_string(),
            version: "0.1.0".to_string(),
            license: None,
            description: None,
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

    #[test]
    fn init_rpm_uses_existing_package_name_override_for_spec_path() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("hello-server/src")).unwrap();
        fs::create_dir_all(dir.path().join("hello-cli/src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"hello-server\", \"hello-cli\"]\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("hello-server/Cargo.toml"),
            "[package]\nname = \"hello-server\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("hello-server/src/main.rs"),
            "fn main() {}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("hello-cli/Cargo.toml"),
            "[package]\nname = \"hello-cli\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("hello-cli/src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            dir.path().join(".cargo-tizen.toml"),
            "# Generated by cargo tizen init.\n\
             [default]\n\
             arch = \"aarch64\"\n\
             profile = \"mobile\"\n\
             platform_version = \"10.0\"\n\
             \n\
             [package]\n\
             name = \"hello-multi\"\n\
             packages = [\"hello-server\", \"hello-cli\"]\n",
        )
        .unwrap();

        let config: Config = basic_toml::from_str(
            &fs::read_to_string(dir.path().join(".cargo-tizen.toml")).unwrap(),
        )
        .unwrap();
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        run_init(
            &ctx,
            &InitArgs {
                rpm: true,
                tpk: false,
                package: None,
                force: false,
            },
        )
        .unwrap();

        let spec_path = dir.path().join("tizen/rpm/hello-multi.spec");
        assert!(spec_path.is_file());
        assert!(!dir.path().join("tizen/rpm/hello-server.spec").exists());

        let spec = fs::read_to_string(spec_path).unwrap();
        assert!(spec.contains("Name:           hello-multi"));
        assert!(spec.contains("Source0:        hello-server"));
        assert!(spec.contains("Source1:        hello-cli"));
    }

    #[test]
    fn init_rpm_errors_when_existing_config_is_missing_package_name() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("demo/src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"demo\"]\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("demo/Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("demo/src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            dir.path().join(".cargo-tizen.toml"),
            "[default]\narch = \"aarch64\"\n\n[package]\npackages = [\"demo\"]\n",
        )
        .unwrap();

        let config: Config = basic_toml::from_str(
            &fs::read_to_string(dir.path().join(".cargo-tizen.toml")).unwrap(),
        )
        .unwrap();
        let ctx = AppContext {
            config,
            workspace_root: dir.path().to_path_buf(),
        };

        let err = run_init(
            &ctx,
            &InitArgs {
                rpm: true,
                tpk: false,
                package: None,
                force: false,
            },
        )
        .expect_err("missing [package].name should fail when generating RPM scaffold")
        .to_string();

        assert!(err.contains("missing [package].name"));
    }

    #[test]
    fn sanitize_identifier_segment_handles_hyphens() {
        assert_eq!(super::sanitize_identifier_segment("my-app"), "my_app");
    }

    #[test]
    fn sanitize_identifier_segment_handles_leading_digit() {
        assert_eq!(super::sanitize_identifier_segment("1app"), "app_1app");
    }

    #[test]
    fn sanitize_identifier_segment_collapses_underscores() {
        assert_eq!(super::sanitize_identifier_segment("a--b"), "a_b");
    }

    #[test]
    fn sanitize_identifier_segment_empty_returns_app() {
        assert_eq!(super::sanitize_identifier_segment(""), "app");
        assert_eq!(super::sanitize_identifier_segment("---"), "app");
    }

    #[test]
    fn title_case_label_converts_hyphenated() {
        assert_eq!(super::title_case_label("my-cool-app"), "My Cool App");
    }

    #[test]
    fn title_case_label_empty_returns_default() {
        assert_eq!(super::title_case_label(""), "Tizen App");
    }

    #[test]
    fn title_case_label_single_word() {
        assert_eq!(super::title_case_label("daemon"), "Daemon");
    }

    #[test]
    fn single_line_joins_multiline_text() {
        assert_eq!(
            super::single_line("line one\n  line two\n\nline three"),
            "line one line two line three"
        );
    }

    #[test]
    fn single_line_handles_single_line() {
        assert_eq!(super::single_line("just one"), "just one");
    }
}
