use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cargo_runner;
use crate::cli::{BuildArgs, TpkArgs};
use crate::context::AppContext;
use crate::sdk::TizenSdk;
use crate::tool_env;

#[derive(Debug, Clone)]
pub struct TpkPackageOutput {
    pub output_dir: PathBuf,
    pub tpk_files: Vec<PathBuf>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: ManifestPackage,
}

#[derive(Debug, Deserialize)]
struct ManifestPackage {
    name: String,
}

pub fn run_tpk(ctx: &AppContext, args: &TpkArgs) -> Result<()> {
    let output = package_tpk(ctx, args)?;
    for tpk in output.tpk_files {
        ctx.info(format!("generated TPK: {}", tpk.display()));
    }
    Ok(())
}

pub fn package_tpk(ctx: &AppContext, args: &TpkArgs) -> Result<TpkPackageOutput> {
    let rust_target = ctx.config.rust_target_for(args.arch);
    let build_target_dir = cargo_runner::resolve_target_dir(&ctx.workspace_root, args.arch, None);

    if !args.no_build {
        let build_args = BuildArgs {
            arch: args.arch,
            release: args.cargo_release,
            target_dir: Some(build_target_dir.clone()),
            cargo_args: Vec::new(),
        };
        cargo_runner::run_build(ctx, &build_args)?;
    }

    let profile_dir = if args.cargo_release {
        "release"
    } else {
        "debug"
    };
    let package_name = manifest_package_name(&ctx.workspace_root.join("Cargo.toml"))?;
    let source_binary = build_target_dir
        .join(&rust_target)
        .join(profile_dir)
        .join(&package_name);
    if !source_binary.is_file() {
        bail!(
            "expected built binary was not found: {}",
            source_binary.display()
        );
    }

    let stage_root = ctx
        .workspace_root
        .join("target")
        .join("tizen")
        .join(args.arch.as_str())
        .join(profile_dir)
        .join("tpk")
        .join("root");
    if stage_root.exists() {
        fs::remove_dir_all(&stage_root)
            .with_context(|| format!("failed to clean staging root {}", stage_root.display()))?;
    }
    fs::create_dir_all(stage_root.join("bin"))
        .with_context(|| format!("failed to create staging root {}", stage_root.display()))?;

    let staged_binary = stage_root.join("bin").join(&package_name);
    fs::copy(&source_binary, &staged_binary).with_context(|| {
        format!(
            "failed to stage binary {} -> {}",
            source_binary.display(),
            staged_binary.display()
        )
    })?;

    let manifest_path = resolve_manifest_path(&ctx.workspace_root, args.manifest.as_deref())?;
    let staged_manifest = stage_root.join("tizen-manifest.xml");
    fs::copy(&manifest_path, &staged_manifest).with_context(|| {
        format!(
            "failed to stage manifest {} -> {}",
            manifest_path.display(),
            staged_manifest.display()
        )
    })?;

    let output_dir = args.output.clone().unwrap_or_else(|| {
        ctx.workspace_root
            .join("target")
            .join("tizen")
            .join(args.arch.as_str())
            .join(profile_dir)
            .join("tpk")
            .join("out")
    });
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create TPK output dir {}", output_dir.display()))?;

    let tizen_cli = locate_tizen_cli(ctx)?;
    ctx.debug(format!("tizen cli resolved to {}", tizen_cli.display()));
    ctx.debug(format!("tpk staging root: {}", stage_root.display()));

    let mut cmd = Command::new(&tizen_cli);
    cmd.arg("package").arg("-t").arg("tpk");
    if let Some(sign) = &args.sign {
        cmd.arg("-s").arg(sign);
    }
    if let Some(reference) = &args.reference {
        cmd.arg("-r").arg(reference);
    }
    if let Some(extra_dir) = &args.extra_dir {
        cmd.arg("-e").arg(extra_dir);
    }
    cmd.arg("-o").arg(&output_dir);
    cmd.arg("--").arg(&stage_root);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);

    ctx.info(format!(
        "running tizen package -t tpk for {} (output: {})",
        args.arch,
        output_dir.display()
    ));
    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {}", tizen_cli.display()))?;
    if !status.success() {
        bail!("tizen package command failed with status: {status}");
    }

    let tpks = collect_tpks(&output_dir)?;
    if tpks.is_empty() {
        bail!(
            "tizen package reported success but no .tpk files were found in {}",
            output_dir.display()
        );
    }

    Ok(TpkPackageOutput {
        output_dir,
        tpk_files: tpks,
        manifest_path,
    })
}

fn manifest_package_name(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Cargo manifest {}", path.display()))?;
    let parsed: CargoManifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse Cargo manifest {}", path.display()))?;
    Ok(parsed.package.name)
}

pub fn resolve_manifest_path(workspace_root: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        bail!("provided manifest path does not exist: {}", path.display());
    }

    let candidates = [
        workspace_root.join("tizen-manifest.xml"),
        workspace_root.join("tizen").join("tizen-manifest.xml"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!(
        "missing tizen-manifest.xml. provide --manifest <path> or place it at {} or {}",
        workspace_root.join("tizen-manifest.xml").display(),
        workspace_root
            .join("tizen")
            .join("tizen-manifest.xml")
            .display()
    )
}

pub fn detect_app_id_from_manifest(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;

    for tag in [
        "ui-application",
        "service-application",
        "watch-application",
        "widget-application",
    ] {
        if let Some(appid) = extract_attr_from_tag(&raw, tag, "appid") {
            return Ok(appid);
        }
    }

    if let Some(pkg) = extract_attr_from_tag(&raw, "manifest", "package") {
        return Ok(pkg);
    }

    bail!(
        "failed to determine app id from {}. pass --app-id explicitly",
        path.display()
    )
}

fn locate_tizen_cli(ctx: &AppContext) -> Result<PathBuf> {
    if let Some(sdk) = TizenSdk::locate(ctx.config.sdk_root().as_deref()) {
        let cli = sdk.tizen_cli();
        if cli.is_file() {
            return Ok(cli);
        }
    }

    if let Ok(path) = which::which("tizen") {
        return Ok(path);
    }

    bail!("unable to locate tizen CLI. install Tizen SDK and configure TIZEN_SDK or [sdk].root")
}

fn collect_tpks(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.is_dir() {
        return Ok(files);
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to list output directory {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_tpks(&path)?);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("tpk") {
            files.push(path);
        }
    }

    files.sort();
    Ok(files)
}

fn extract_attr_from_tag(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let needle = format!("<{tag}");
    let mut from = 0usize;
    while from < xml.len() {
        let rel = xml[from..].find(&needle)?;
        let start = from + rel;
        let end_rel = xml[start..].find('>')?;
        let end = start + end_rel + 1;
        let segment = &xml[start..end];
        if let Some(value) = extract_attr(segment, attr) {
            return Some(value);
        }
        from = end;
    }
    None
}

fn extract_attr(segment: &str, attr: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let needle = attr.as_bytes();
    let mut i = 0usize;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] != needle {
            i += 1;
            continue;
        }

        if i > 0 {
            let prev = bytes[i - 1] as char;
            if prev.is_ascii_alphanumeric() || prev == '_' || prev == '-' || prev == ':' {
                i += 1;
                continue;
            }
        }

        let mut j = i + needle.len();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'=' {
            i += 1;
            continue;
        }

        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || (bytes[j] != b'"' && bytes[j] != b'\'') {
            i += 1;
            continue;
        }
        let quote = bytes[j];
        j += 1;
        let start = j;
        while j < bytes.len() && bytes[j] != quote {
            j += 1;
        }
        if j <= bytes.len() {
            return Some(String::from_utf8_lossy(&bytes[start..j]).to_string());
        }

        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{extract_attr, extract_attr_from_tag};

    #[test]
    fn xml_attr_parser_handles_spaces_and_quotes() {
        let tag = r#"<ui-application appid = "org.example.app" exec='demo' />"#;
        assert_eq!(
            extract_attr(tag, "appid").as_deref(),
            Some("org.example.app")
        );
        assert_eq!(extract_attr(tag, "exec").as_deref(), Some("demo"));
    }

    #[test]
    fn xml_tag_lookup_finds_appid() {
        let manifest = r#"
<manifest package="org.example.package">
  <ui-application appid="org.example.app" />
</manifest>
"#;
        assert_eq!(
            extract_attr_from_tag(manifest, "ui-application", "appid").as_deref(),
            Some("org.example.app")
        );
    }
}
