use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::arch_detect;
use crate::cargo_runner;
use crate::cli::{BuildArgs, TpkArgs};
use crate::context::AppContext;
use crate::rust_target;
use crate::sdk::TizenSdk;
use crate::sysroot;
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
    version: String,
}

pub fn run_tpk(ctx: &AppContext, args: &TpkArgs) -> Result<()> {
    let output = package_tpk(ctx, args)?;
    for tpk in output.tpk_files {
        ctx.info(format!("generated TPK: {}", tpk.display()));
    }
    Ok(())
}

pub fn package_tpk(ctx: &AppContext, args: &TpkArgs) -> Result<TpkPackageOutput> {
    let arch = arch_detect::resolve_arch(ctx, args.arch, "tpk")?;
    let rust_target = rust_target::resolve_for_arch(ctx, arch)?;
    let build_target_dir = cargo_runner::resolve_target_dir(&ctx.workspace_root, arch, None);

    if !args.no_build {
        let build_args = BuildArgs {
            arch: Some(arch),
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
    let package = manifest_package(&ctx.workspace_root.join("Cargo.toml"))?;
    let package_name = package.name.clone();
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
        .join(arch.as_str())
        .join(profile_dir)
        .join("tpk")
        .join("root");
    if stage_root.exists() {
        fs::remove_dir_all(&stage_root)
            .with_context(|| format!("failed to clean staging root {}", stage_root.display()))?;
    }
    fs::create_dir_all(&stage_root)
        .with_context(|| format!("failed to create staging root {}", stage_root.display()))?;

    let staged_manifest = stage_root.join("tizen-manifest.xml");
    let manifest_path = stage_manifest(
        ctx,
        &ctx.workspace_root,
        args.manifest.as_deref(),
        &staged_manifest,
        arch,
        &package,
    )?;

    let output_dir = args.output.clone().unwrap_or_else(|| {
        ctx.workspace_root
            .join("target")
            .join("tizen")
            .join(arch.as_str())
            .join(profile_dir)
            .join("tpk")
            .join("out")
    });
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create TPK output dir {}", output_dir.display()))?;

    let tizen_cli = locate_tizen_cli(ctx)?;
    ctx.debug(format!("tizen cli resolved to {}", tizen_cli.display()));
    let (resolved_profile, platform_version) = match sysroot::resolve_profile_platform_for_arch(
        ctx, arch,
    ) {
        Ok(resolved) => resolved,
        Err(err) => {
            ctx.debug(format!(
                "profile/platform auto-detection failed for tpk project: {}; falling back to config defaults",
                err
            ));
            (ctx.config.profile(), ctx.config.platform_version())
        }
    };

    let template_profile = tizen_template_profile_name(&platform_version);
    let temp_project_root = ctx
        .workspace_root
        .join("target")
        .join("tizen")
        .join(arch.as_str())
        .join(profile_dir)
        .join("tpk")
        .join("project");
    if temp_project_root.exists() {
        fs::remove_dir_all(&temp_project_root).with_context(|| {
            format!(
                "failed to clean temporary tpk project root {}",
                temp_project_root.display()
            )
        })?;
    }
    fs::create_dir_all(&temp_project_root).with_context(|| {
        format!(
            "failed to create temporary tpk project root {}",
            temp_project_root.display()
        )
    })?;

    let temp_project_name = format!("ctpk_{}", sanitize_identifier_segment(&package_name));
    let temp_project_dir = create_temp_native_project(
        ctx,
        &tizen_cli,
        &temp_project_root,
        &template_profile,
        &temp_project_name,
    )?;
    let temp_manifest = temp_project_dir.join("tizen-manifest.xml");
    fs::copy(&staged_manifest, &temp_manifest).with_context(|| {
        format!(
            "failed to stage manifest {} -> {}",
            staged_manifest.display(),
            temp_manifest.display()
        )
    })?;

    let build_config = if args.cargo_release {
        "Release"
    } else {
        "Debug"
    };
    let build_output_dir =
        build_temp_native_project(ctx, &tizen_cli, &temp_project_dir, arch, build_config)?;

    let exec_name = detect_exec_name_from_manifest(&staged_manifest)?.unwrap_or(package_name);
    let packaged_binary = build_output_dir.join(&exec_name);
    fs::copy(&source_binary, &packaged_binary).with_context(|| {
        format!(
            "failed to inject Rust binary {} -> {}",
            source_binary.display(),
            packaged_binary.display()
        )
    })?;
    ensure_executable_mode(&packaged_binary)?;

    let resolved_sign = args.sign.as_deref().or_else(|| ctx.config.tpk_sign());

    ctx.info(format!(
        "running tizen package -t tpk for {} profile={} platform-version={} (output: {})",
        arch,
        resolved_profile,
        platform_version,
        output_dir.display()
    ));
    run_tizen_package(
        ctx,
        &tizen_cli,
        resolved_sign,
        args,
        &output_dir,
        &build_output_dir,
    )?;

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

fn manifest_package(path: &Path) -> Result<ManifestPackage> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Cargo manifest {}", path.display()))?;
    let parsed: CargoManifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse Cargo manifest {}", path.display()))?;
    Ok(parsed.package)
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

fn stage_manifest(
    ctx: &AppContext,
    workspace_root: &Path,
    explicit: Option<&Path>,
    staged_manifest: &Path,
    arch: crate::arch::Arch,
    package: &ManifestPackage,
) -> Result<PathBuf> {
    if explicit.is_some() {
        let manifest_path = resolve_manifest_path(workspace_root, explicit)?;
        fs::copy(&manifest_path, staged_manifest).with_context(|| {
            format!(
                "failed to stage manifest {} -> {}",
                manifest_path.display(),
                staged_manifest.display()
            )
        })?;
        return Ok(manifest_path);
    }

    if let Ok(manifest_path) = resolve_manifest_path(workspace_root, None) {
        fs::copy(&manifest_path, staged_manifest).with_context(|| {
            format!(
                "failed to stage manifest {} -> {}",
                manifest_path.display(),
                staged_manifest.display()
            )
        })?;
        return Ok(manifest_path);
    }

    let (profile, platform_version) = match sysroot::resolve_profile_platform_for_arch(ctx, arch) {
        Ok(resolved) => resolved,
        Err(err) => {
            ctx.debug(format!(
                "manifest profile/platform auto-detection failed: {}; falling back to config defaults",
                err
            ));
            (ctx.config.profile(), ctx.config.platform_version())
        }
    };
    let rendered = render_default_manifest(package, &profile, &platform_version);
    fs::write(staged_manifest, rendered).with_context(|| {
        format!(
            "failed to write generated manifest {}",
            staged_manifest.display()
        )
    })?;
    ctx.info(format!(
        "no tizen-manifest.xml found; generated default manifest at {}",
        staged_manifest.display()
    ));
    Ok(staged_manifest.to_path_buf())
}

fn render_default_manifest(
    package: &ManifestPackage,
    profile: &str,
    platform_version: &str,
) -> String {
    let id_segment = sanitize_identifier_segment(&package.name);
    let package_id = format!("org.rust.{id_segment}");
    let manifest_version = normalize_manifest_version(&package.version);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns="http://tizen.org/ns/packages" package="{package_id}" version="{manifest_version}" api-version="{platform_version}">
    <profile name="{profile}" />
    <service-application appid="{appid}" exec="{exec}" type="capp" multiple="false" taskmanage="false">
        <label>{label}</label>
    </service-application>
    <privileges>
        <privilege>http://tizen.org/privilege/appmanager.launch</privilege>
        <privilege>http://tizen.org/privilege/filesystem.read</privilege>
        <privilege>http://tizen.org/privilege/filesystem.write</privilege>
        <privilege>http://tizen.org/privilege/network.get</privilege>
        <privilege>http://tizen.org/privilege/network.set</privilege>
        <privilege>http://tizen.org/privilege/internet</privilege>
        <privilege>http://tizen.org/privilege/externalstorage</privilege>
        <privilege>http://tizen.org/privilege/externalstorage.appdata</privilege>
        <privilege>http://tizen.org/privilege/mediastorage</privilege>
        <privilege>http://tizen.org/privilege/appdir.shareddata</privilege>
    </privileges>
</manifest>
"#,
        package_id = package_id,
        manifest_version = manifest_version,
        platform_version = platform_version,
        label = package.name,
        profile = profile,
        appid = package_id,
        exec = package.name,
    )
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
    let out = out.trim_matches('_');

    if out.is_empty() {
        return "app".to_string();
    }

    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("app_{out}");
    }

    out.to_string()
}

fn normalize_manifest_version(raw: &str) -> String {
    let core = raw.split(['-', '+']).next().unwrap_or("").trim();
    let mut parts: Vec<u64> = core
        .split('.')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect();

    if parts.is_empty() {
        return "1.0.0".to_string();
    }

    while parts.len() < 3 {
        parts.push(0);
    }
    parts.truncate(3);

    format!("{}.{}.{}", parts[0], parts[1], parts[2])
}

fn create_temp_native_project(
    ctx: &AppContext,
    tizen_cli: &Path,
    project_root: &Path,
    template_profile: &str,
    project_name: &str,
) -> Result<PathBuf> {
    let mut cmd = Command::new(tizen_cli);
    cmd.arg("create")
        .arg("native-project")
        .arg("-p")
        .arg(template_profile)
        .arg("-t")
        .arg("ServiceApp")
        .arg("-n")
        .arg(project_name)
        .arg("--")
        .arg(project_root);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);

    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {}", tizen_cli.display()))?;
    if !status.success() {
        bail!("tizen create native-project failed with status: {status}");
    }

    let project_dir = project_root.join(project_name);
    if !project_dir.is_dir() {
        bail!(
            "tizen create native-project reported success but project directory is missing: {}",
            project_dir.display()
        );
    }
    Ok(project_dir)
}

fn build_temp_native_project(
    ctx: &AppContext,
    tizen_cli: &Path,
    project_dir: &Path,
    arch: crate::arch::Arch,
    build_config: &str,
) -> Result<PathBuf> {
    let mut cmd = Command::new(tizen_cli);
    cmd.arg("build-native")
        .arg("-a")
        .arg(ctx.config.tizen_cli_arch_for(arch))
        .arg("-C")
        .arg(build_config)
        .arg("--")
        .arg(project_dir);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);

    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {}", tizen_cli.display()))?;
    if !status.success() {
        bail!("tizen build-native failed with status: {status}");
    }

    let output_dir = project_dir.join(build_config);
    if !output_dir.is_dir() {
        bail!(
            "tizen build-native reported success but output directory is missing: {}",
            output_dir.display()
        );
    }
    Ok(output_dir)
}

fn run_tizen_package(
    ctx: &AppContext,
    tizen_cli: &Path,
    sign: Option<&str>,
    args: &TpkArgs,
    output_dir: &Path,
    build_output_dir: &Path,
) -> Result<()> {
    let mut cmd = Command::new(tizen_cli);
    cmd.arg("package").arg("-t").arg("tpk");
    if let Some(sign) = sign {
        cmd.arg("-s").arg(sign);
    }
    if let Some(reference) = &args.reference {
        cmd.arg("-r").arg(reference);
    }
    if let Some(extra_dir) = &args.extra_dir {
        cmd.arg("-e").arg(extra_dir);
    }
    cmd.arg("-o").arg(output_dir);
    cmd.arg("--").arg(build_output_dir);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);

    let status = cmd
        .status()
        .with_context(|| format!("failed to execute {}", tizen_cli.display()))?;
    if !status.success() {
        bail!("tizen package command failed with status: {status}");
    }

    Ok(())
}

fn detect_exec_name_from_manifest(path: &Path) -> Result<Option<String>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    for tag in [
        "ui-application",
        "service-application",
        "watch-application",
        "widget-application",
    ] {
        if let Some(exec) = extract_attr_from_tag(&raw, tag, "exec") {
            return Ok(Some(exec));
        }
    }
    Ok(None)
}

fn ensure_executable_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)
            .with_context(|| format!("failed to read metadata {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .with_context(|| format!("failed to set executable mode on {}", path.display()))?;
    }

    Ok(())
}

fn tizen_template_profile_name(platform_version: &str) -> String {
    if platform_version.starts_with("tizen-") {
        platform_version.to_string()
    } else {
        format!("tizen-{platform_version}")
    }
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
    let doc = match roxmltree::Document::parse(xml) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("warning: failed to parse manifest XML: {e}");
            return None;
        }
    };
    doc.descendants()
        .find(|n| n.tag_name().name() == tag)
        .and_then(|n| n.attribute(attr))
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::{
        ManifestPackage, extract_attr_from_tag, normalize_manifest_version,
        render_default_manifest, sanitize_identifier_segment, tizen_template_profile_name,
    };

    #[test]
    fn xml_attr_parser_handles_spaces_and_quotes() {
        // roxmltree normalizes whitespace around `=` and handles both quote
        // styles, so we exercise the same scenarios the old hand-rolled
        // parser was tested against.
        let xml = r#"<?xml version="1.0"?><manifest><ui-application appid = "org.example.app" exec='demo' /></manifest>"#;
        assert_eq!(
            extract_attr_from_tag(xml, "ui-application", "appid").as_deref(),
            Some("org.example.app")
        );
        assert_eq!(
            extract_attr_from_tag(xml, "ui-application", "exec").as_deref(),
            Some("demo")
        );
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

    #[test]
    fn default_manifest_is_rendered_with_normalized_fields() {
        let package = ManifestPackage {
            name: "my-app".to_string(),
            version: "0.9.2-beta.1".to_string(),
        };
        let manifest = render_default_manifest(&package, "tizen", "10.0");
        assert!(manifest.contains(r#"package="org.rust.my_app""#));
        assert!(manifest.contains(r#"appid="org.rust.my_app""#));
        assert!(manifest.contains(r#"version="0.9.2""#));
        assert!(manifest.contains(r#"api-version="10.0""#));
        assert!(manifest.contains(r#"profile name="tizen""#));
        assert!(manifest.contains("service-application"));
        assert!(!manifest.contains("ui-application"));
        assert!(manifest.contains("http://tizen.org/privilege/internet"));
    }

    #[test]
    fn identifier_segment_sanitization_is_stable() {
        assert_eq!(sanitize_identifier_segment("my-app"), "my_app");
        assert_eq!(sanitize_identifier_segment("99-app"), "app_99_app");
        assert_eq!(sanitize_identifier_segment("__"), "app");
    }

    #[test]
    fn manifest_version_normalization_is_stable() {
        assert_eq!(normalize_manifest_version("1"), "1.0.0");
        assert_eq!(normalize_manifest_version("1.2"), "1.2.0");
        assert_eq!(normalize_manifest_version("1.2.3-beta.1"), "1.2.3");
        assert_eq!(normalize_manifest_version("abc"), "1.0.0");
    }

    #[test]
    fn template_profile_name_is_normalized() {
        assert_eq!(tizen_template_profile_name("10.0"), "tizen-10.0");
        assert_eq!(tizen_template_profile_name("tizen-9.0"), "tizen-9.0");
    }
}
