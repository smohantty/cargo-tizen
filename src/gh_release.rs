use std::collections::HashSet;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::arch::Arch;
use crate::cargo_runner;
use crate::cli::{BuildArgs, BumpLevel, GhReleaseArgs, RpmArgs};
use crate::config::Config;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};
use crate::package_select::{self, SelectedPackage};
use crate::packaging::PackagingLayout;
use crate::rpm;

const RELEASE_REMOTE: &str = "origin";
const RELEASE_BRANCH: &str = "main";

struct ResolvedConfig {
    packages: Vec<SelectedPackage>,
    arches: Vec<Arch>,
    spec_name: String,
    tag_format: String,
}

struct ReleasePlan {
    package_name: String,
    packages: Vec<String>,
    version: String,
    tag: String,
    arches: Vec<Arch>,
    workspace_root: PathBuf,
    packaging_root: PathBuf,
    version_bumped: bool,
    cargo_toml_paths: Vec<PathBuf>,
    notes: String,
    reuse_tag: bool,
}

struct ReleaseVersion {
    paths: Vec<PathBuf>,
    version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RpmStageKey {
    name: String,
    arch: String,
}

pub fn run_gh_release(ctx: &AppContext, args: &GhReleaseArgs) -> Result<()> {
    validate_flags(args)?;

    let release_ctx = load_project_context(ctx)?;
    let resolved = resolve_config(&release_ctx, args)?;
    preflight_checks()?;

    let packaging = PackagingLayout::new(
        &release_ctx.workspace_root,
        release_ctx.config.packaging_dir().as_deref(),
    );
    let spec_path = packaging.resolve_rpm_spec(&resolved.spec_name)?;

    let mut version_bumped = false;
    let mut cargo_toml_paths = Vec::new();
    let version = if let Some(level) = args.bump {
        let release_version =
            resolve_release_version(&release_ctx.workspace_root, &resolved.packages)?;
        let current_version = release_version.version.clone();
        let new_version = bump_version(&current_version, level)?;
        if !args.dry_run {
            for toml_path in &release_version.paths {
                update_cargo_toml_version(toml_path, &current_version, &new_version)?;
            }
            cargo_toml_paths = release_version.paths;
            version_bumped = true;

            let use_color = color_enabled();
            ctx.info(format!(
                "{} {} -> {}",
                cargo_status(use_color, "Bumped"),
                current_version,
                new_version
            ));
        }
        new_version
    } else {
        read_release_version(&release_ctx.workspace_root, &resolved.packages)?
    };

    let spec_synced = sync_spec_if_needed(args.dry_run, &spec_path, &version)?;
    let tag = format_tag(&resolved.tag_format, &version);
    if !args.reuse_tag && (check_tag_exists(&tag) || remote_tag_exists(RELEASE_REMOTE, &tag)?) {
        bail!(
            "tag {} already exists\n\
             bump the version, remove the existing tag, or pass --reuse-tag to force-move it to HEAD",
            tag
        );
    }

    let notes = generate_release_notes(&release_ctx.workspace_root, &resolved.tag_format, &tag)?;
    let plan = ReleasePlan {
        package_name: resolved.spec_name.clone(),
        packages: resolved
            .packages
            .iter()
            .map(|pkg| pkg.name.clone())
            .collect(),
        version: version.clone(),
        tag: tag.clone(),
        arches: resolved.arches.clone(),
        workspace_root: release_ctx.workspace_root.clone(),
        packaging_root: packaging.root().to_path_buf(),
        version_bumped,
        cargo_toml_paths,
        notes,
        reuse_tag: args.reuse_tag,
    };

    print_plan(&plan, spec_synced);

    if args.dry_run {
        return Ok(());
    }
    if !args.yes && !prompt_yn("Proceed?", true) {
        bail!("aborted by user");
    }

    let use_color = color_enabled();
    let build_cargo_args = cargo_package_args(&resolved.packages);

    for &arch in &plan.arches {
        ctx.info(format!(
            "{} {} (release)",
            cargo_status(use_color, "Building"),
            arch.as_str()
        ));
        let build_args = BuildArgs {
            arch: Some(arch),
            release: true,
            target_dir: Some(cargo_runner::resolve_target_dir(
                &release_ctx.workspace_root,
                arch,
                None,
            )),
            cargo_args: build_cargo_args.clone(),
        };
        cargo_runner::run_build(&release_ctx, &build_args)?;
    }

    let mut all_rpms = Vec::new();
    for &arch in &plan.arches {
        ctx.info(format!(
            "{} {} (release, --no-build)",
            cargo_status(use_color, "Packaging RPM"),
            arch.as_str()
        ));
        let rpm_args = RpmArgs {
            arch: Some(arch),
            package: None,
            release: true,
            packaging_dir: None,
            output: None,
            no_build: true,
        };
        rpm::run_rpm(&release_ctx, &rpm_args)?;

        let rpm_arch = release_ctx.config.rpm_build_arch_for(arch);
        let mut rpms = collect_rpm_artifacts(&plan.workspace_root, arch, &rpm_arch, &plan.version)?;
        all_rpms.append(&mut rpms);
    }

    if all_rpms.is_empty() {
        bail!("no RPM files found after packaging");
    }

    stage_rpms(ctx, &plan, &all_rpms)?;

    let mut all_assets = Vec::new();
    for rpm in &all_rpms {
        let sidecar = generate_sha256_sidecar(rpm)?;
        ctx.info(format!(
            "{} {}",
            cargo_status(use_color, "SHA256"),
            sidecar.file_name().unwrap_or_default().to_string_lossy()
        ));
        all_assets.push(rpm.clone());
        all_assets.push(sidecar);
    }

    let mut paths_to_add = Vec::new();
    for toml_path in &plan.cargo_toml_paths {
        let toml_rel = toml_path
            .strip_prefix(&plan.workspace_root)
            .unwrap_or(toml_path);
        paths_to_add.push(toml_rel.to_string_lossy().to_string());
    }
    if !plan.cargo_toml_paths.is_empty() {
        let lock_path = plan.workspace_root.join("Cargo.lock");
        if lock_path.is_file() {
            paths_to_add.push("Cargo.lock".to_string());
        }
    }
    let sources_dir = plan.packaging_root.join("rpm").join("sources");
    let sources_rel = sources_dir
        .strip_prefix(&plan.workspace_root)
        .unwrap_or(&sources_dir);
    paths_to_add.push(format!("{}/", sources_rel.display()));
    if spec_synced {
        let spec_rel = spec_path
            .strip_prefix(&plan.workspace_root)
            .unwrap_or(&spec_path);
        paths_to_add.push(spec_rel.to_string_lossy().to_string());
    }
    if !paths_to_add.is_empty() {
        git_commit(&plan, &paths_to_add)?;
    }

    if plan.reuse_tag {
        git_force_tag_and_push(&plan)?;
    } else {
        git_tag_and_push(&plan)?;
    }
    gh_release_create(&plan, &all_assets, &plan.notes)?;
    verify_release(&plan, &all_assets)?;
    print_summary(ctx, &plan, &all_assets);

    Ok(())
}

fn validate_flags(args: &GhReleaseArgs) -> Result<()> {
    if args.reuse_tag && args.bump.is_some() {
        bail!(
            "--reuse-tag cannot be combined with --bump\n\
             --reuse-tag republishes the same version's tag (force-moved to HEAD); --bump changes the version"
        );
    }
    Ok(())
}

fn load_project_context(ctx: &AppContext) -> Result<AppContext> {
    let project_config_path = ctx.workspace_root.join(".cargo-tizen.toml");
    if !project_config_path.is_file() {
        bail!(
            "gh-release requires project config at {}\n\
             add .cargo-tizen.toml with [package].name and [package].packages",
            project_config_path.display()
        );
    }

    let config = Config::read_path(&project_config_path)?;
    Ok(AppContext {
        config,
        workspace_root: ctx.workspace_root.clone(),
    })
}

fn resolve_config(ctx: &AppContext, args: &GhReleaseArgs) -> Result<ResolvedConfig> {
    let arches = if !args.arch.is_empty() {
        args.arch.clone()
    } else if let Some(ref configured) = ctx.config.release.arches {
        parse_release_arches(configured)?
    } else {
        vec![Arch::Armv7l, Arch::Aarch64]
    };

    let packages = package_select::resolve_configured_packages(ctx)?.ok_or_else(|| {
        anyhow::anyhow!(
            "gh-release requires [package].packages in .cargo-tizen.toml\n\
             define the exact crates to build and stage for release"
        )
    })?;
    let spec_name = ctx.config.package.name().ok_or_else(|| {
        anyhow::anyhow!(
            "gh-release requires [package].name in .cargo-tizen.toml\n\
             use it to define the RPM/spec and release artifact name"
        )
    })?;
    let tag_format = resolve_tag_format(ctx.config.release.tag_format.as_deref())?;

    Ok(ResolvedConfig {
        packages,
        arches,
        spec_name: spec_name.to_string(),
        tag_format,
    })
}

fn parse_release_arches(configured: &[String]) -> Result<Vec<Arch>> {
    if configured.is_empty() {
        bail!("[release].arches must not be empty");
    }

    let mut arches = Vec::with_capacity(configured.len());
    let mut invalid = Vec::new();
    for raw in configured {
        match Arch::parse(raw) {
            Some(arch) => arches.push(arch),
            None => invalid.push(raw.clone()),
        }
    }

    if !invalid.is_empty() {
        bail!(
            "invalid architecture(s) in [release].arches: {}\n\
             expected one or more of: armv7l, aarch64",
            invalid.join(", ")
        );
    }

    Ok(arches)
}

fn resolve_tag_format(configured: Option<&str>) -> Result<String> {
    let tag_format = configured.unwrap_or("v{version}").trim();
    if tag_format.is_empty() {
        bail!("[release].tag_format must not be empty");
    }
    if !tag_format.contains("{version}") {
        bail!("[release].tag_format must contain {{version}}");
    }
    Ok(tag_format.to_string())
}

fn preflight_checks() -> Result<()> {
    which::which("git").context("git not found in PATH")?;
    which::which("gh").context(
        "gh not found in PATH\n\
         install from: https://cli.github.com",
    )?;

    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .context("failed to run git status")?;
    let status_text = String::from_utf8_lossy(&output.stdout);
    if !status_text.trim().is_empty() {
        bail!(
            "working tree is not clean — commit or stash changes first\n\n{}",
            status_text.trim()
        );
    }

    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .context("failed to determine current branch")?;
    let current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if current_branch != RELEASE_BRANCH {
        bail!(
            "releases must be created from branch {} (current: {})",
            RELEASE_BRANCH,
            if current_branch.is_empty() {
                "detached HEAD"
            } else {
                &current_branch
            }
        );
    }

    let status = Command::new("gh")
        .args(["auth", "status"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to check gh auth status")?;
    if !status.success() {
        bail!("gh is not authenticated — run: gh auth login");
    }

    let status = Command::new("git")
        .args(["remote", "get-url", RELEASE_REMOTE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to check git remote")?;
    if !status.success() {
        bail!("git remote not found: {}", RELEASE_REMOTE);
    }

    Ok(())
}

fn cargo_package_args(packages: &[SelectedPackage]) -> Vec<String> {
    let mut cargo_args = Vec::new();
    for package in packages {
        if package.source.requires_cargo_package_arg() {
            cargo_args.extend(["-p".to_string(), package.name.clone()]);
        }
    }
    cargo_args
}

#[cfg(test)]
fn default_package_name(workspace_root: &Path) -> String {
    let toml_path = workspace_root.join("Cargo.toml");
    let Ok(raw) = fs::read_to_string(&toml_path) else {
        return String::new();
    };
    let Ok(parsed) = basic_toml::from_str::<toml_types::CargoToml>(&raw) else {
        return String::new();
    };
    parsed.package.and_then(|p| p.name).unwrap_or_default()
}

fn resolve_cargo_version_path(
    workspace_root: &Path,
    package_name: &str,
) -> Result<(PathBuf, String)> {
    let toml_path = workspace_root.join("Cargo.toml");
    let parsed = read_cargo_toml(&toml_path)?;

    let member_toml =
        find_package_manifest_path(workspace_root, package_name).unwrap_or_else(|| {
            if parsed.package.as_ref().and_then(|pkg| pkg.name.as_deref()) == Some(package_name) {
                toml_path.clone()
            } else {
                workspace_root.join(package_name).join("Cargo.toml")
            }
        });
    if member_toml.is_file() {
        let member = read_cargo_toml(&member_toml)?;
        if let Some(ref pkg) = member.package {
            if let Some(version) = pkg
                .version
                .as_ref()
                .and_then(toml_types::ManifestVersion::as_literal)
            {
                return Ok((member_toml.clone(), version.to_string()));
            }
            if pkg
                .version
                .as_ref()
                .is_some_and(toml_types::ManifestVersion::uses_workspace)
            {
                if let Some(ref ws) = parsed.workspace {
                    if let Some(ref pkg) = ws.package {
                        if let Some(version) = pkg
                            .version
                            .as_ref()
                            .and_then(toml_types::ManifestVersion::as_literal)
                        {
                            return Ok((toml_path.clone(), version.to_string()));
                        }
                    }
                }
            }
        }
    }

    if let Some(ref ws) = parsed.workspace {
        if let Some(ref pkg) = ws.package {
            if let Some(version) = pkg
                .version
                .as_ref()
                .and_then(toml_types::ManifestVersion::as_literal)
            {
                return Ok((toml_path.clone(), version.to_string()));
            }
        }
    }

    bail!(
        "no version found for package `{}`\n\
         checked {} and {}\n\
         expected [package].version or [workspace.package].version",
        package_name,
        toml_path.display(),
        member_toml.display()
    )
}

#[cfg(test)]
fn read_cargo_version(workspace_root: &Path, package_name: &str) -> Result<String> {
    resolve_cargo_version_path(workspace_root, package_name).map(|(_, version)| version)
}

fn read_cargo_toml(path: &Path) -> Result<toml_types::CargoToml> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    basic_toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn resolve_release_version(
    workspace_root: &Path,
    packages: &[SelectedPackage],
) -> Result<ReleaseVersion> {
    let mut resolved = Vec::new();
    for package in packages {
        let (path, version) = resolve_cargo_version_path(workspace_root, &package.name)?;
        resolved.push((package.name.clone(), path, version));
    }

    let Some((_, _, version)) = resolved.first() else {
        bail!("gh-release requires at least one package");
    };
    let version = version.clone();

    if resolved
        .iter()
        .any(|(_, _, package_version)| package_version != &version)
    {
        let details = resolved
            .iter()
            .map(|(name, path, package_version)| {
                format!("{name}={package_version} ({})", path.display())
            })
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "gh-release requires all configured packages to share one release version\n\
             resolved packages: {}",
            details
        );
    }

    let mut paths = Vec::new();
    for (_, path, _) in resolved {
        if paths.iter().all(|existing| existing != &path) {
            paths.push(path);
        }
    }

    Ok(ReleaseVersion { paths, version })
}

fn read_release_version(workspace_root: &Path, packages: &[SelectedPackage]) -> Result<String> {
    resolve_release_version(workspace_root, packages).map(|release| release.version)
}

fn find_package_manifest_path(workspace_root: &Path, package_name: &str) -> Option<PathBuf> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .current_dir(workspace_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let metadata: toml_types::CargoMetadata = serde_json::from_slice(&output.stdout).ok()?;
    metadata
        .packages
        .into_iter()
        .find(|package| package.name == package_name)
        .map(|package| PathBuf::from(package.manifest_path))
}

fn bump_version(current: &str, level: BumpLevel) -> Result<String> {
    let parts: Vec<&str> = current.split('.').collect();
    if parts.len() != 3 {
        bail!(
            "version '{}' is not valid semver (expected MAJOR.MINOR.PATCH)",
            current
        );
    }
    let major: u64 = parts[0]
        .parse()
        .with_context(|| format!("invalid major version: {}", parts[0]))?;
    let minor: u64 = parts[1]
        .parse()
        .with_context(|| format!("invalid minor version: {}", parts[1]))?;
    let patch: u64 = parts[2]
        .parse()
        .with_context(|| format!("invalid patch version: {}", parts[2]))?;

    let (major, minor, patch) = match level {
        BumpLevel::Major => (major + 1, 0, 0),
        BumpLevel::Minor => (major, minor + 1, 0),
        BumpLevel::Patch => (major, minor, patch + 1),
    };

    Ok(format!("{}.{}.{}", major, minor, patch))
}

fn update_cargo_toml_version(toml_path: &Path, old_version: &str, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(toml_path)
        .with_context(|| format!("failed to read {}", toml_path.display()))?;

    let mut lines = Vec::new();
    let mut in_target_section = false;
    let mut updated = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            in_target_section = trimmed == "[package]" || trimmed == "[workspace.package]";
        }

        if in_target_section && !updated {
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim();
                    if rest.starts_with('"') && rest.contains(old_version) {
                        let indent = &line[..line.len() - line.trim_start().len()];
                        lines.push(format!("{}version = \"{}\"", indent, new_version));
                        updated = true;
                        continue;
                    }
                }
            }
        }

        lines.push(line.to_string());
    }

    if !updated {
        bail!(
            "could not find version = \"{}\" in {} under [package] or [workspace.package]",
            old_version,
            toml_path.display()
        );
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    fs::write(toml_path, output)
        .with_context(|| format!("failed to write {}", toml_path.display()))?;

    Ok(())
}

fn read_spec_version(spec_path: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(spec_path)
        .with_context(|| format!("failed to read {}", spec_path.display()))?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Version:") {
            return Ok(Some(rest.trim().to_string()));
        }
    }
    Ok(None)
}

fn sync_spec_version(spec_path: &Path, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(spec_path)
        .with_context(|| format!("failed to read {}", spec_path.display()))?;
    let mut lines = Vec::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("Version:") && !found {
            lines.push(format!("Version:        {}", new_version));
            found = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        bail!("no Version: field found in {}", spec_path.display());
    }
    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    fs::write(spec_path, output)
        .with_context(|| format!("failed to write {}", spec_path.display()))?;
    Ok(())
}

fn sync_spec_if_needed(dry_run: bool, spec_path: &Path, new_version: &str) -> Result<bool> {
    let current_version = read_spec_version(spec_path)?.ok_or_else(|| {
        anyhow::anyhow!(
            "RPM spec is missing a Version: field: {}",
            spec_path.display()
        )
    })?;
    if current_version == new_version {
        return Ok(false);
    }
    if !dry_run {
        sync_spec_version(spec_path, new_version)?;
    }
    Ok(true)
}

fn format_tag(format: &str, version: &str) -> String {
    format.replace("{version}", version)
}

fn check_tag_exists(tag: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/tags/{}", tag)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn remote_tag_exists(remote: &str, tag: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", remote, &format!("refs/tags/{}", tag)])
        .output()
        .context("failed to check remote tag")?;
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn print_plan(plan: &ReleasePlan, spec_synced: bool) {
    let use_color = color_enabled();
    let packaging_rel = plan
        .packaging_root
        .strip_prefix(&plan.workspace_root)
        .unwrap_or(&plan.packaging_root);
    let arch_list: Vec<&str> = plan.arches.iter().map(|arch| arch.as_str()).collect();

    println!();
    println!(
        "{} {} {}",
        cargo_status(use_color, "gh-release"),
        plan.package_name,
        plan.tag
    );
    println!("     Packages: {}", plan.packages.join(", "));
    if plan.version_bumped {
        println!("     Bump:     version -> {}", plan.version);
    }
    println!("     Build:    {} (release)", arch_list.join(", "));
    for arch in &plan.arches {
        println!(
            "     RPM:      {}-{}-1.{}.rpm",
            plan.package_name,
            plan.version,
            arch.rpm_arch()
        );
        println!(
            "     Stage:    {}/rpm/sources/{}-{}-1.{}.rpm",
            packaging_rel.display(),
            plan.package_name,
            plan.version,
            arch.rpm_arch()
        );
    }
    if spec_synced {
        println!("     Spec:     Version: updated to {}", plan.version);
    }
    println!(
        "     Commit:   \"{}\"",
        if plan.version_bumped {
            format!(
                "Bump version to {} and update release artifacts for {}",
                plan.version, plan.tag
            )
        } else {
            format!("Update release artifacts for {}", plan.tag)
        }
    );
    println!(
        "     Tag:      {} ({})",
        plan.tag,
        if plan.reuse_tag {
            "force-move to HEAD"
        } else {
            "new"
        }
    );
    println!(
        "     Push:     {}/HEAD + {}tag {}",
        RELEASE_REMOTE,
        if plan.reuse_tag { "force-push " } else { "" },
        plan.tag
    );
    println!(
        "     Release:  GitHub release {} with RPM + SHA256 assets",
        plan.tag
    );
    println!("     Notes:");
    for line in plan.notes.lines() {
        println!("       {}", line);
    }
    println!();
}

fn prompt_yn(question: &str, default_yes: bool) -> bool {
    if !std::io::stdin().is_terminal() {
        return default_yes;
    }
    let hint = if default_yes { "Y/n" } else { "y/N" };
    eprint!("{} [{}] ", question, hint);
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return default_yes;
    }
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        default_yes
    } else {
        input == "y" || input == "yes"
    }
}

fn stage_rpms(ctx: &AppContext, plan: &ReleasePlan, rpms: &[PathBuf]) -> Result<()> {
    let use_color = color_enabled();
    let dest_dir = plan.packaging_root.join("rpm").join("sources");
    fs::create_dir_all(&dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;
    let staged_keys = rpm_stage_keys(rpms)?;

    for entry in
        fs::read_dir(&dest_dir).with_context(|| format!("failed to read {}", dest_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?
            .is_file()
        {
            continue;
        }

        let Some(key) = parse_rpm_stage_key(&path) else {
            continue;
        };
        if !staged_keys.contains(&key) {
            continue;
        }

        fs::remove_file(&path)
            .with_context(|| format!("failed to remove stale staged RPM {}", path.display()))?;
        let path_rel = path.strip_prefix(&plan.workspace_root).unwrap_or(&path);
        ctx.info(format!(
            "{} {}",
            cargo_status(use_color, "Removed"),
            path_rel.display(),
        ));
    }

    for rpm in rpms {
        let filename = rpm
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid RPM path: {}", rpm.display()))?;
        let dest = dest_dir.join(filename);
        fs::copy(rpm, &dest).with_context(|| {
            format!(
                "failed to stage RPM {} -> {}",
                rpm.display(),
                dest.display()
            )
        })?;

        let dest_rel = dest.strip_prefix(&plan.workspace_root).unwrap_or(&dest);
        ctx.info(format!(
            "{} {}",
            cargo_status(use_color, "Staged"),
            dest_rel.display(),
        ));
    }

    Ok(())
}

fn rpm_stage_keys(rpms: &[PathBuf]) -> Result<HashSet<RpmStageKey>> {
    rpms.iter()
        .map(|rpm| {
            parse_rpm_stage_key(rpm)
                .ok_or_else(|| anyhow::anyhow!("invalid RPM filename: {}", rpm.display()))
        })
        .collect()
}

fn parse_rpm_stage_key(path: &Path) -> Option<RpmStageKey> {
    let filename = path.file_name()?.to_str()?;
    let stem = filename.strip_suffix(".rpm")?;
    let (nvra, arch) = stem.rsplit_once('.')?;
    let mut parts = nvra.rsplitn(3, '-');
    let release = parts.next()?;
    let version = parts.next()?;
    let name = parts.next()?;
    if name.is_empty() || version.is_empty() || release.is_empty() || arch.is_empty() {
        return None;
    }

    Some(RpmStageKey {
        name: name.to_string(),
        arch: arch.to_string(),
    })
}

fn collect_rpm_artifacts(
    workspace_root: &Path,
    arch: Arch,
    rpm_arch: &str,
    version: &str,
) -> Result<Vec<PathBuf>> {
    let rpm_dir = workspace_root
        .join("target/tizen")
        .join(arch.as_str())
        .join("release/rpmbuild/RPMS")
        .join(rpm_arch);

    if !rpm_dir.is_dir() {
        return Ok(Vec::new());
    }

    let version_needle = format!("-{}-", version);
    let mut rpms = Vec::new();
    for entry in fs::read_dir(&rpm_dir)
        .with_context(|| format!("failed to read RPM directory {}", rpm_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("rpm") {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.contains(&version_needle) {
                rpms.push(path);
            }
        }
    }
    rpms.sort();
    Ok(rpms)
}

fn generate_sha256_sidecar(rpm_path: &Path) -> Result<PathBuf> {
    let data =
        fs::read(rpm_path).with_context(|| format!("failed to read {}", rpm_path.display()))?;
    let hash = Sha256::digest(&data);
    let filename = rpm_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid RPM path"))?
        .to_string_lossy();

    let sidecar_name = format!("{}.sha256", filename);
    let sidecar_path = rpm_path.with_file_name(&sidecar_name);
    fs::write(&sidecar_path, format!("{:x}  {}\n", hash, filename))
        .with_context(|| format!("failed to write {}", sidecar_path.display()))?;

    Ok(sidecar_path)
}

fn tag_match_pattern(tag_format: &str) -> String {
    tag_format.replace("{version}", "*")
}

fn previous_release_tag(
    workspace_root: &Path,
    tag_format: &str,
    current_tag: &str,
) -> Result<Option<String>> {
    let points_at_head = Command::new("git")
        .args(["tag", "--points-at", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .context("failed to inspect tags on HEAD")?;
    let tags_on_head = String::from_utf8_lossy(&points_at_head.stdout);
    let start_rev = if tags_on_head.lines().any(|tag| tag.trim() == current_tag) {
        "HEAD^"
    } else {
        "HEAD"
    };

    let output = Command::new("git")
        .args([
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            &tag_match_pattern(tag_format),
            start_rev,
        ])
        .current_dir(workspace_root)
        .output()
        .context("failed to determine previous release tag")?;
    if !output.status.success() {
        return Ok(None);
    }

    let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tag.is_empty() {
        Ok(None)
    } else {
        Ok(Some(tag))
    }
}

fn generate_release_notes(
    workspace_root: &Path,
    tag_format: &str,
    current_tag: &str,
) -> Result<String> {
    let previous_tag = previous_release_tag(workspace_root, tag_format, current_tag)?;

    let mut cmd = Command::new("git");
    cmd.arg("log")
        .arg("--pretty=- %s")
        .current_dir(workspace_root);
    if let Some(ref previous_tag) = previous_tag {
        cmd.arg(format!("{}..HEAD", previous_tag));
    }

    let output = cmd.output().context("failed to generate release notes")?;
    if !output.status.success() {
        bail!("git log failed while generating release notes");
    }

    let notes = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if notes.is_empty() {
        Ok(format!("Release {}", current_tag))
    } else {
        Ok(notes)
    }
}

fn git_commit(plan: &ReleasePlan, paths: &[String]) -> Result<()> {
    let mut add_args = vec!["add".to_string()];
    add_args.extend(paths.iter().cloned());

    let status = Command::new("git")
        .args(&add_args)
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to run git add")?;
    if !status.success() {
        bail!("git add failed");
    }

    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&plan.workspace_root)
        .status();
    if let Ok(status) = staged {
        if status.success() {
            return Ok(());
        }
    }

    let message = if plan.version_bumped {
        format!(
            "Bump version to {} and update release artifacts for {}",
            plan.version, plan.tag
        )
    } else {
        format!("Update release artifacts for {}", plan.tag)
    };
    let status = Command::new("git")
        .args(["commit", "-m", &message])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to run git commit")?;
    if !status.success() {
        bail!("git commit failed");
    }

    Ok(())
}

fn git_tag_and_push(plan: &ReleasePlan) -> Result<()> {
    let tag_message = format!("Release {}", plan.tag);
    let status = Command::new("git")
        .args(["tag", "-a", &plan.tag, "-m", &tag_message])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to create git tag")?;
    if !status.success() {
        bail!("git tag failed");
    }

    let status = Command::new("git")
        .args(["push", RELEASE_REMOTE, "HEAD"])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to push branch")?;
    if !status.success() {
        bail!(
            "git push failed\n\
             the tag {} was created locally — you may need to push manually",
            plan.tag
        );
    }

    let tag_ref = format!("refs/tags/{}", plan.tag);
    let status = Command::new("git")
        .args(["push", RELEASE_REMOTE, &tag_ref])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to push tag")?;
    if !status.success() {
        bail!("git push tag {} failed", plan.tag);
    }

    Ok(())
}

fn git_force_tag_and_push(plan: &ReleasePlan) -> Result<()> {
    let tag_message = format!("Release {}", plan.tag);
    let status = Command::new("git")
        .args(["tag", "-fa", &plan.tag, "-m", &tag_message])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to create or move git tag")?;
    if !status.success() {
        bail!("git tag -f {} failed", plan.tag);
    }

    let status = Command::new("git")
        .args(["push", RELEASE_REMOTE, "HEAD"])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to push branch")?;
    if !status.success() {
        bail!(
            "git push failed\n\
             the tag {} was moved locally — push the branch and tag manually",
            plan.tag
        );
    }

    let tag_ref = format!("refs/tags/{}", plan.tag);
    let status = Command::new("git")
        .args(["push", "--force", RELEASE_REMOTE, &tag_ref])
        .current_dir(&plan.workspace_root)
        .status()
        .context("failed to force-push tag")?;
    if !status.success() {
        bail!("git push --force tag {} failed", plan.tag);
    }

    Ok(())
}

fn gh_release_create(plan: &ReleasePlan, assets: &[PathBuf], notes: &str) -> Result<()> {
    let notes_file =
        std::env::temp_dir().join(format!("cargo-tizen-notes-{}.md", std::process::id()));
    fs::write(&notes_file, notes).context("failed to write temp notes file")?;

    let existing = Command::new("gh")
        .args(["release", "view", &plan.tag])
        .current_dir(&plan.workspace_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if existing {
        let status = Command::new("gh")
            .args([
                "release",
                "edit",
                &plan.tag,
                "--title",
                &plan.tag,
                "--notes-file",
                &notes_file.to_string_lossy(),
            ])
            .current_dir(&plan.workspace_root)
            .status()
            .context("failed to run gh release edit")?;
        if !status.success() {
            let _ = fs::remove_file(&notes_file);
            bail!("gh release edit failed");
        }

        let mut upload_args = vec![
            "release".to_string(),
            "upload".to_string(),
            plan.tag.clone(),
            "--clobber".to_string(),
        ];
        for asset in assets {
            upload_args.push(asset.to_string_lossy().to_string());
        }
        let status = Command::new("gh")
            .args(&upload_args)
            .current_dir(&plan.workspace_root)
            .status()
            .context("failed to run gh release upload")?;
        if !status.success() {
            let _ = fs::remove_file(&notes_file);
            bail!("gh release upload failed");
        }
    } else {
        let mut create_args = vec![
            "release".to_string(),
            "create".to_string(),
            plan.tag.clone(),
            "--title".to_string(),
            plan.tag.clone(),
            "--notes-file".to_string(),
            notes_file.to_string_lossy().to_string(),
        ];
        for asset in assets {
            create_args.push(asset.to_string_lossy().to_string());
        }
        let status = Command::new("gh")
            .args(&create_args)
            .current_dir(&plan.workspace_root)
            .status()
            .context("failed to run gh release create")?;
        if !status.success() {
            let _ = fs::remove_file(&notes_file);
            bail!(
                "gh release create failed\n\
                 artifacts are committed and tagged — run gh release create manually"
            );
        }
    }

    let _ = fs::remove_file(&notes_file);
    Ok(())
}

fn verify_release(plan: &ReleasePlan, assets: &[PathBuf]) -> Result<()> {
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&plan.workspace_root)
        .output()
        .context("failed to get HEAD")?;
    let head_commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

    let remote_tag = Command::new("git")
        .args([
            "ls-remote",
            RELEASE_REMOTE,
            &format!("refs/tags/{}^{{}}", plan.tag),
        ])
        .current_dir(&plan.workspace_root)
        .output()
        .context("failed to check remote tag")?;
    let remote_output = String::from_utf8_lossy(&remote_tag.stdout);
    let remote_commit = remote_output.split_whitespace().next().unwrap_or("");

    if !remote_commit.is_empty() && remote_commit != head_commit {
        bail!(
            "remote tag {} does not resolve to HEAD\n\
             expected: {}\n\
             got:      {}",
            plan.tag,
            head_commit,
            remote_commit
        );
    }

    let output = Command::new("gh")
        .args([
            "release",
            "view",
            &plan.tag,
            "--json",
            "assets",
            "--jq",
            ".assets[].name",
        ])
        .current_dir(&plan.workspace_root)
        .output()
        .context("failed to check release assets")?;
    let release_assets = String::from_utf8_lossy(&output.stdout);

    for asset in assets {
        let name = asset
            .file_name()
            .map(|file_name| file_name.to_string_lossy().to_string())
            .unwrap_or_default();
        if !release_assets.contains(&name) {
            bail!(
                "release asset missing after upload: {}\n\
                 check the release page manually",
                name
            );
        }
    }

    Ok(())
}

fn print_summary(ctx: &AppContext, plan: &ReleasePlan, assets: &[PathBuf]) {
    let use_color = color_enabled();
    let url = Command::new("gh")
        .args([
            "release", "view", &plan.tag, "--json", "url", "--jq", ".url",
        ])
        .current_dir(&plan.workspace_root)
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();

    println!();
    ctx.info(format!(
        "{} {}",
        cargo_status(use_color, "Released"),
        plan.tag
    ));

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&plan.workspace_root)
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    ctx.info(format!("  commit: {}", head));

    if !url.is_empty() {
        ctx.info(format!("  url:    {}", url));
    }

    for asset in assets {
        if let Some(name) = asset.file_name() {
            ctx.info(format!("  asset:  {}", name.to_string_lossy()));
        }
    }
}

mod toml_types {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct CargoToml {
        pub package: Option<PackageSection>,
        pub workspace: Option<WorkspaceSection>,
    }

    #[derive(Debug, Deserialize)]
    pub struct PackageSection {
        #[cfg_attr(not(test), allow(dead_code))]
        pub name: Option<String>,
        pub version: Option<ManifestVersion>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WorkspaceSection {
        pub package: Option<WorkspacePackage>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WorkspacePackage {
        pub version: Option<ManifestVersion>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub enum ManifestVersion {
        Literal(String),
        Inherited(InheritedValue),
    }

    impl ManifestVersion {
        pub fn as_literal(&self) -> Option<&str> {
            match self {
                Self::Literal(version) => Some(version.as_str()),
                Self::Inherited(_) => None,
            }
        }

        pub fn uses_workspace(&self) -> bool {
            matches!(self, Self::Inherited(InheritedValue { workspace: true }))
        }
    }

    #[derive(Debug, Deserialize)]
    pub struct InheritedValue {
        pub workspace: bool,
    }

    #[derive(Debug, Deserialize)]
    pub struct CargoMetadata {
        pub packages: Vec<MetadataPackage>,
    }

    #[derive(Debug, Deserialize)]
    pub struct MetadataPackage {
        pub name: String,
        pub manifest_path: String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::config::Config;

    #[test]
    fn read_cargo_version_from_package() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        let version = read_cargo_version(dir.path(), "test").unwrap();
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn read_cargo_version_from_workspace() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        let version = read_cargo_version(dir.path(), "a").unwrap();
        assert_eq!(version, "0.5.0");
    }

    #[test]
    fn read_cargo_version_from_workspace_inherited_member() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("a")).unwrap();
        fs::write(
            dir.path().join("a").join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion.workspace = true\n",
        )
        .unwrap();
        let version = read_cargo_version(dir.path(), "a").unwrap();
        assert_eq!(version, "0.5.0");
    }

    #[test]
    fn read_cargo_version_missing_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n",
        )
        .unwrap();
        assert!(read_cargo_version(dir.path(), "a").is_err());
    }

    #[test]
    fn read_spec_version_found() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name: test\nVersion:        0.1.0\nRelease: 1\n").unwrap();
        let version = read_spec_version(&spec).unwrap();
        assert_eq!(version, Some("0.1.0".to_string()));
    }

    #[test]
    fn read_spec_version_missing() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name: test\nRelease: 1\n").unwrap();
        let version = read_spec_version(&spec).unwrap();
        assert_eq!(version, None);
    }

    #[test]
    fn format_tag_default() {
        assert_eq!(format_tag("v{version}", "0.2.0"), "v0.2.0");
    }

    #[test]
    fn format_tag_custom() {
        assert_eq!(format_tag("release-{version}", "1.0.0"), "release-1.0.0");
    }

    #[test]
    fn format_tag_no_placeholder() {
        assert_eq!(format_tag("fixed-tag", "1.0.0"), "fixed-tag");
    }

    #[test]
    fn resolve_tag_format_defaults_to_v_prefix() {
        assert_eq!(resolve_tag_format(None).unwrap(), "v{version}");
    }

    #[test]
    fn resolve_tag_format_rejects_missing_placeholder() {
        assert!(resolve_tag_format(Some("enterprise")).is_err());
    }

    #[test]
    fn validate_flags_ok() {
        let args = GhReleaseArgs {
            arch: vec![],
            bump: None,
            dry_run: false,
            yes: false,
            reuse_tag: false,
        };
        assert!(validate_flags(&args).is_ok());
    }

    #[test]
    fn validate_flags_rejects_reuse_tag_with_bump() {
        let args = GhReleaseArgs {
            arch: vec![],
            bump: Some(BumpLevel::Patch),
            dry_run: false,
            yes: false,
            reuse_tag: true,
        };
        let err = validate_flags(&args).unwrap_err().to_string();
        assert!(err.contains("--reuse-tag"));
        assert!(err.contains("--bump"));
    }

    #[test]
    fn validate_flags_accepts_reuse_tag_alone() {
        let args = GhReleaseArgs {
            arch: vec![],
            bump: None,
            dry_run: false,
            yes: false,
            reuse_tag: true,
        };
        assert!(validate_flags(&args).is_ok());
    }

    #[test]
    fn generate_sha256_produces_correct_hash() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rpm");
        fs::write(&file, b"hello world").unwrap();

        let sidecar = generate_sha256_sidecar(&file).unwrap();
        assert!(sidecar.exists());

        let content = fs::read_to_string(&sidecar).unwrap();
        assert!(
            content.starts_with("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
        assert!(content.contains("test.rpm"));
    }

    #[test]
    fn stage_rpms_replaces_only_matching_package_and_arch() {
        let dir = tempfile::tempdir().unwrap();
        let workspace_root = dir.path();
        let packaging_root = workspace_root.join("tizen");
        let sources_dir = packaging_root.join("rpm").join("sources");
        fs::create_dir_all(&sources_dir).unwrap();

        fs::write(
            sources_dir.join("hello-service-0.1.0-1.aarch64.rpm"),
            b"old aarch64",
        )
        .unwrap();
        fs::write(
            sources_dir.join("hello-service-0.1.0-1.armv7l.rpm"),
            b"old armv7l",
        )
        .unwrap();
        fs::write(
            sources_dir.join("hello-service-helper-9.9.9-1.aarch64.rpm"),
            b"external helper",
        )
        .unwrap();
        fs::write(
            sources_dir.join("external-lib-2.0.0-1.aarch64.rpm"),
            b"external lib",
        )
        .unwrap();
        fs::write(sources_dir.join("hello-service.env"), b"ENV=prod").unwrap();

        let built_dir = workspace_root.join("target").join("test-rpms");
        fs::create_dir_all(&built_dir).unwrap();
        let new_rpm = built_dir.join("hello-service-0.2.0-1.aarch64.rpm");
        fs::write(&new_rpm, b"new aarch64").unwrap();

        let ctx = AppContext {
            config: Config::default(),
            workspace_root: workspace_root.to_path_buf(),
        };
        let plan = ReleasePlan {
            package_name: "hello-service".to_string(),
            packages: vec!["hello-service".to_string()],
            version: "0.2.0".to_string(),
            tag: "v0.2.0".to_string(),
            arches: vec![Arch::Aarch64],
            workspace_root: workspace_root.to_path_buf(),
            packaging_root,
            version_bumped: false,
            cargo_toml_paths: Vec::new(),
            notes: String::new(),
            reuse_tag: false,
        };

        stage_rpms(&ctx, &plan, &[new_rpm]).unwrap();

        assert!(
            !sources_dir
                .join("hello-service-0.1.0-1.aarch64.rpm")
                .exists()
        );
        assert_eq!(
            fs::read(sources_dir.join("hello-service-0.2.0-1.aarch64.rpm")).unwrap(),
            b"new aarch64"
        );
        assert_eq!(
            fs::read(sources_dir.join("hello-service-0.1.0-1.armv7l.rpm")).unwrap(),
            b"old armv7l"
        );
        assert_eq!(
            fs::read(sources_dir.join("hello-service-helper-9.9.9-1.aarch64.rpm")).unwrap(),
            b"external helper"
        );
        assert_eq!(
            fs::read(sources_dir.join("external-lib-2.0.0-1.aarch64.rpm")).unwrap(),
            b"external lib"
        );
        assert_eq!(
            fs::read(sources_dir.join("hello-service.env")).unwrap(),
            b"ENV=prod"
        );
    }

    #[test]
    fn sync_spec_version_updates_field() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name: test\nVersion:        0.1.0\nRelease: 1\n").unwrap();
        sync_spec_version(&spec, "0.2.0").unwrap();
        let content = fs::read_to_string(&spec).unwrap();
        assert!(content.contains("Version:        0.2.0"));
        assert!(!content.contains("0.1.0"));
    }

    #[test]
    fn sync_spec_if_needed_skips_write_in_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name: test\nVersion:        0.1.0\nRelease: 1\n").unwrap();
        let changed = sync_spec_if_needed(true, &spec, "0.2.0").unwrap();
        assert!(changed);
        assert!(fs::read_to_string(&spec).unwrap().contains("0.1.0"));
    }

    #[test]
    fn default_package_name_reads_package() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(default_package_name(dir.path()), "my-app");
    }

    #[test]
    fn default_package_name_empty_for_workspace() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n",
        )
        .unwrap();
        assert_eq!(default_package_name(dir.path()), "");
    }

    #[test]
    fn bump_version_patch() {
        assert_eq!(bump_version("1.2.3", BumpLevel::Patch).unwrap(), "1.2.4");
    }

    #[test]
    fn bump_version_minor() {
        assert_eq!(bump_version("1.2.3", BumpLevel::Minor).unwrap(), "1.3.0");
    }

    #[test]
    fn bump_version_major() {
        assert_eq!(bump_version("1.2.3", BumpLevel::Major).unwrap(), "2.0.0");
    }

    #[test]
    fn bump_version_invalid() {
        assert!(bump_version("not-a-version", BumpLevel::Patch).is_err());
    }

    #[test]
    fn update_cargo_toml_version_in_package() {
        let dir = tempfile::tempdir().unwrap();
        let toml = dir.path().join("Cargo.toml");
        fs::write(&toml, "[package]\nname = \"test\"\nversion = \"1.2.3\"\n").unwrap();
        update_cargo_toml_version(&toml, "1.2.3", "1.3.0").unwrap();
        let content = fs::read_to_string(&toml).unwrap();
        assert!(content.contains("version = \"1.3.0\""));
        assert!(!content.contains("1.2.3"));
    }

    #[test]
    fn update_cargo_toml_version_in_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let toml = dir.path().join("Cargo.toml");
        fs::write(
            &toml,
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        update_cargo_toml_version(&toml, "0.5.0", "0.6.0").unwrap();
        let content = fs::read_to_string(&toml).unwrap();
        assert!(content.contains("version = \"0.6.0\""));
        assert!(!content.contains("0.5.0"));
    }

    #[test]
    fn update_cargo_toml_version_in_workspace_package_for_inherited_root_package() {
        let dir = tempfile::tempdir().unwrap();
        let toml = dir.path().join("Cargo.toml");
        fs::write(
            &toml,
            "[package]\nname = \"demo\"\nversion.workspace = true\n\n[workspace]\nmembers = [\".\"]\n\n[workspace.package]\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        update_cargo_toml_version(&toml, "0.5.0", "0.6.0").unwrap();
        let content = fs::read_to_string(&toml).unwrap();
        assert!(content.contains("version.workspace = true"));
        assert!(content.contains("[workspace.package]\nversion = \"0.6.0\""));
        assert!(!content.contains("version = \"0.5.0\""));
    }

    #[test]
    fn update_cargo_toml_version_preserves_formatting() {
        let dir = tempfile::tempdir().unwrap();
        let toml = dir.path().join("Cargo.toml");
        let original = "[package]\nname = \"test\"\nversion = \"1.0.0\"\nedition = \"2021\"\n";
        fs::write(&toml, original).unwrap();
        update_cargo_toml_version(&toml, "1.0.0", "1.1.0").unwrap();
        let content = fs::read_to_string(&toml).unwrap();
        assert!(content.contains("name = \"test\""));
        assert!(content.contains("edition = \"2021\""));
        assert!(content.contains("version = \"1.1.0\""));
    }

    #[test]
    fn resolve_cargo_version_path_returns_correct_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"2.0.0\"\n",
        )
        .unwrap();
        let (path, version) = resolve_cargo_version_path(dir.path(), "test").unwrap();
        assert_eq!(path, dir.path().join("Cargo.toml"));
        assert_eq!(version, "2.0.0");
    }

    #[test]
    fn resolve_cargo_version_path_returns_workspace_file_for_inherited_member() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"device/rsdbd\"]\n\n[workspace.package]\nversion = \"0.1.3\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("device").join("rsdbd")).unwrap();
        fs::write(
            dir.path().join("device").join("rsdbd").join("Cargo.toml"),
            "[package]\nname = \"rsdbd\"\nversion.workspace = true\n",
        )
        .unwrap();

        let (path, version) = resolve_cargo_version_path(dir.path(), "rsdbd").unwrap();
        assert_eq!(path, dir.path().join("Cargo.toml"));
        assert_eq!(version, "0.1.3");
    }

    #[test]
    fn resolve_cargo_version_path_accepts_inline_workspace_inheritance() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"server\"]\n\n[workspace.package]\nversion = \"2.0.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("server")).unwrap();
        fs::write(
            dir.path().join("server").join("Cargo.toml"),
            "[package]\nname = \"server\"\nversion = { workspace = true }\n",
        )
        .unwrap();

        let (path, version) = resolve_cargo_version_path(dir.path(), "server").unwrap();
        assert_eq!(path, dir.path().join("Cargo.toml"));
        assert_eq!(version, "2.0.0");
    }

    #[test]
    fn resolve_release_version_accepts_matching_versions_from_multiple_manifests() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"server\", \"cli\"]\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("server")).unwrap();
        fs::create_dir_all(dir.path().join("cli")).unwrap();
        fs::write(
            dir.path().join("server").join("Cargo.toml"),
            "[package]\nname = \"server\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("cli").join("Cargo.toml"),
            "[package]\nname = \"cli\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let packages = vec![
            SelectedPackage {
                name: "server".to_string(),
                source: package_select::PackageSource::Config,
            },
            SelectedPackage {
                name: "cli".to_string(),
                source: package_select::PackageSource::Config,
            },
        ];

        let release = resolve_release_version(dir.path(), &packages).unwrap();
        assert_eq!(release.version, "1.0.0");
        assert_eq!(
            release.paths,
            vec![
                dir.path().join("server").join("Cargo.toml"),
                dir.path().join("cli").join("Cargo.toml"),
            ]
        );
    }

    #[test]
    fn resolve_release_version_rejects_mismatched_versions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"server\", \"cli\"]\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("server")).unwrap();
        fs::create_dir_all(dir.path().join("cli")).unwrap();
        fs::write(
            dir.path().join("server").join("Cargo.toml"),
            "[package]\nname = \"server\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("cli").join("Cargo.toml"),
            "[package]\nname = \"cli\"\nversion = \"2.0.0\"\n",
        )
        .unwrap();

        let packages = vec![
            SelectedPackage {
                name: "server".to_string(),
                source: package_select::PackageSource::Config,
            },
            SelectedPackage {
                name: "cli".to_string(),
                source: package_select::PackageSource::Config,
            },
        ];

        assert!(resolve_release_version(dir.path(), &packages).is_err());
    }

    #[test]
    fn resolve_release_version_accepts_shared_workspace_version_source() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"server\", \"cli\"]\n\n[workspace.package]\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("server")).unwrap();
        fs::create_dir_all(dir.path().join("cli")).unwrap();
        fs::write(
            dir.path().join("server").join("Cargo.toml"),
            "[package]\nname = \"server\"\nversion.workspace = true\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("cli").join("Cargo.toml"),
            "[package]\nname = \"cli\"\nversion.workspace = true\n",
        )
        .unwrap();

        let packages = vec![
            SelectedPackage {
                name: "server".to_string(),
                source: package_select::PackageSource::Config,
            },
            SelectedPackage {
                name: "cli".to_string(),
                source: package_select::PackageSource::Config,
            },
        ];

        let release = resolve_release_version(dir.path(), &packages).unwrap();
        assert_eq!(release.version, "1.2.3");
        assert_eq!(release.paths, vec![dir.path().join("Cargo.toml")]);
    }

    #[test]
    fn generate_release_notes_uses_previous_release_tag_range() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), ["init", "-b", "main"]);
        git(dir.path(), ["config", "user.email", "dev@example.com"]);
        git(dir.path(), ["config", "user.name", "Dev"]);

        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("src.rs"), "fn main() {}\n").unwrap();
        git(dir.path(), ["add", "."]);
        git(dir.path(), ["commit", "-m", "initial"]);
        git(dir.path(), ["tag", "-a", "v0.1.0", "-m", "Release v0.1.0"]);

        fs::write(
            dir.path().join("src.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();
        git(dir.path(), ["add", "."]);
        git(dir.path(), ["commit", "-m", "feature one"]);

        fs::write(dir.path().join("README.md"), "demo\n").unwrap();
        git(dir.path(), ["add", "."]);
        git(dir.path(), ["commit", "-m", "feature two"]);

        let notes = generate_release_notes(dir.path(), "v{version}", "v0.2.0").unwrap();
        assert!(notes.contains("- feature one"));
        assert!(notes.contains("- feature two"));
        assert!(!notes.contains("initial"));
    }

    #[test]
    fn generate_release_notes_respects_custom_tag_namespace() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), ["init", "-b", "main"]);
        git(dir.path(), ["config", "user.email", "dev@example.com"]);
        git(dir.path(), ["config", "user.name", "Dev"]);

        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("src.rs"), "fn main() {}\n").unwrap();
        git(dir.path(), ["add", "."]);
        git(dir.path(), ["commit", "-m", "initial"]);
        git(
            dir.path(),
            [
                "tag",
                "-a",
                "upstream-v0.1.0",
                "-m",
                "Release upstream-v0.1.0",
            ],
        );
        git(
            dir.path(),
            [
                "tag",
                "-a",
                "enterprise-v0.1.0",
                "-m",
                "Release enterprise-v0.1.0",
            ],
        );

        fs::write(
            dir.path().join("src.rs"),
            "fn main() { println!(\"enterprise\"); }\n",
        )
        .unwrap();
        git(dir.path(), ["add", "."]);
        git(dir.path(), ["commit", "-m", "enterprise fix"]);

        let notes =
            generate_release_notes(dir.path(), "enterprise-v{version}", "enterprise-v0.2.0")
                .unwrap();
        assert!(notes.contains("- enterprise fix"));
        assert!(!notes.contains("initial"));
    }

    #[test]
    fn parse_rpm_stage_key_extracts_name_and_arch() {
        let key = parse_rpm_stage_key(std::path::Path::new("my-app-1.0.0-1.aarch64.rpm"));
        assert!(key.is_some());
        let key = key.unwrap();
        assert_eq!(key.name, "my-app");
        assert_eq!(key.arch, "aarch64");
    }

    #[test]
    fn parse_rpm_stage_key_returns_none_for_non_rpm() {
        assert!(parse_rpm_stage_key(std::path::Path::new("readme.txt")).is_none());
    }

    #[test]
    fn parse_rpm_stage_key_returns_none_for_insufficient_parts() {
        assert!(parse_rpm_stage_key(std::path::Path::new("short.rpm")).is_none());
    }

    fn git(dir: &Path, args: impl IntoIterator<Item = &'static str>) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success());
    }
}
