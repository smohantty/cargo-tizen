use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::arch::Arch;
use crate::cli::{BumpLevel, GhReleaseArgs};
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled};
use crate::packaging::PackagingLayout;

// ---------------------------------------------------------------------------
// Resolved configuration (CLI args merged with .cargo-tizen.toml)
// ---------------------------------------------------------------------------

struct ResolvedConfig {
    package: Option<String>,
    arches: Vec<Arch>,
    remote: String,
    branch: String,
    tag_format: String,
    sync_spec_version: bool,
}

// ---------------------------------------------------------------------------
// Release plan (everything needed to execute the release)
// ---------------------------------------------------------------------------

struct ReleasePlan {
    package_name: String,
    version: String,
    tag: String,
    arches: Vec<Arch>,
    remote: String,
    no_stage: bool,
    no_gh_release: bool,
    draft: bool,
    notes_file: Option<PathBuf>,
    notes_command: Option<String>,
    workspace_root: PathBuf,
    packaging_root: PathBuf,
    version_bumped: bool,
    cargo_toml_path: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run_gh_release(ctx: &AppContext, args: &GhReleaseArgs) -> Result<()> {
    validate_flags(args)?;

    let resolved = resolve_config(ctx, args);

    // -- Preflight checks --
    preflight_checks(&resolved.remote, &resolved.branch)?;

    // -- Resolve package name --
    let package_name = resolve_package_name(ctx, args, &resolved)?;

    // -- Resolve packaging layout --
    let packaging =
        PackagingLayout::new(&ctx.workspace_root, ctx.config.packaging_dir().as_deref());

    // -- Version bump (optional) --
    let mut version_bumped = false;
    let mut cargo_toml_path: Option<PathBuf> = None;
    let version = if let Some(level) = args.bump {
        let (toml_path, current_version) =
            resolve_cargo_version_path(&ctx.workspace_root, &package_name)?;
        let new_version = bump_version(&current_version, level)?;
        if !args.dry_run {
            update_cargo_toml_version(&toml_path, &current_version, &new_version)?;
            cargo_toml_path = Some(toml_path);
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
        read_cargo_version(&ctx.workspace_root, &package_name)?
    };

    // -- Spec version sync --
    let spec_name = ctx.config.rpm_spec_name().unwrap_or(&package_name);
    let spec_path = packaging.rpm_spec_path(spec_name);
    let mut spec_synced = false;
    if spec_path.is_file() {
        let spec_version = read_spec_version(&spec_path)?;
        if let Some(ref sv) = spec_version {
            if sv != &version {
                if version_bumped
                    || args.sync_spec_version
                    || resolved.sync_spec_version
                    || args.yes
                {
                    sync_spec_version(&spec_path, &version)?;
                    spec_synced = true;
                } else {
                    let prompt = format!(
                        "Cargo.toml version is {} but spec says {}. Update spec to match?",
                        version, sv
                    );
                    if prompt_yn(&prompt, true) {
                        sync_spec_version(&spec_path, &version)?;
                        spec_synced = true;
                    }
                }
            }
        }
    }

    // -- Tag resolution --
    let tag = format_tag(&resolved.tag_format, &version);
    let tag_exists = check_tag_exists(&tag);

    // -- Force-tag gating --
    if tag_exists && !args.force_tag {
        if args.yes {
            bail!("tag {} already exists — use --force-tag to re-tag", tag);
        }
        let prompt = format!("Tag {} already exists. Force-move it to HEAD?", tag);
        if !prompt_yn(&prompt, false) {
            bail!("aborted: tag {} already exists", tag);
        }
    }

    // -- Build plan --
    let plan = ReleasePlan {
        package_name: spec_name.to_string(),
        version: version.clone(),
        tag: tag.clone(),
        arches: resolved.arches.clone(),
        remote: resolved.remote.clone(),
        no_stage: args.no_stage,
        no_gh_release: args.no_gh_release,
        draft: args.draft,
        notes_file: args.notes_file.clone(),
        notes_command: ctx.config.gh_release.notes_command.clone(),
        workspace_root: ctx.workspace_root.clone(),
        packaging_root: packaging.root().to_path_buf(),
        version_bumped,
        cargo_toml_path,
    };

    // -- Print plan (always, like an implicit dry-run) --
    print_plan(&plan, spec_synced, tag_exists);

    // -- Confirm --
    if args.dry_run {
        return Ok(());
    }
    if !args.yes {
        if !prompt_yn("Proceed?", true) {
            bail!("aborted by user");
        }
    }

    // ======================================================================
    // PHASE B: Execute
    // ======================================================================

    let use_color = color_enabled();
    let exe = self_exe()?;

    // Step 3: Cross-build
    // The package is already resolved from .cargo-tizen.toml [default].package.
    // Just pass -A and --release; let the existing config handle the rest.
    for &arch in &plan.arches {
        ctx.info(format!(
            "{} {} (release)",
            cargo_status(use_color, "Building"),
            arch.as_str()
        ));
        run_cargo_tizen(&exe, &["build", "-A", arch.as_str(), "--release"])?;
    }

    // Step 4: Build RPMs
    let mut all_rpms: Vec<PathBuf> = Vec::new();
    for &arch in &plan.arches {
        ctx.info(format!(
            "{} {} (release, --no-build)",
            cargo_status(use_color, "Packaging RPM"),
            arch.as_str()
        ));
        run_cargo_tizen(
            &exe,
            &["rpm", "-A", arch.as_str(), "--release", "--no-build"],
        )?;

        let rpm_arch = ctx.config.rpm_build_arch_for(arch);
        let mut rpms = collect_rpm_artifacts(&plan.workspace_root, arch, &rpm_arch, &plan.version)?;
        all_rpms.append(&mut rpms);
    }

    if all_rpms.is_empty() {
        bail!("no RPM files found after packaging");
    }

    // Step 5: Stage RPMs to tizen/rpm/
    if !plan.no_stage {
        stage_rpms(ctx, &plan, &all_rpms)?;
    }

    // Step 6: SHA256 sidecars
    let mut all_assets: Vec<PathBuf> = Vec::new();
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

    // Step 7: Commit version bump, staged RPMs, and spec changes
    {
        let mut paths_to_add: Vec<String> = Vec::new();
        if let Some(ref toml_path) = plan.cargo_toml_path {
            let toml_rel = toml_path
                .strip_prefix(&plan.workspace_root)
                .unwrap_or(toml_path);
            paths_to_add.push(toml_rel.to_string_lossy().to_string());

            // Cargo.lock is updated when Cargo.toml version changes; include it
            // in the commit if it exists.
            let lock_path = plan.workspace_root.join("Cargo.lock");
            if lock_path.is_file() {
                paths_to_add.push("Cargo.lock".to_string());
            }
        }
        if !plan.no_stage {
            let sources_dir = plan.packaging_root.join("rpm").join("sources");
            let sources_rel = sources_dir
                .strip_prefix(&plan.workspace_root)
                .unwrap_or(&sources_dir);
            paths_to_add.push(format!("{}/", sources_rel.display()));
        }
        if spec_synced {
            let spec_rel = spec_path
                .strip_prefix(&plan.workspace_root)
                .unwrap_or(&spec_path);
            paths_to_add.push(spec_rel.to_string_lossy().to_string());
        }
        if !paths_to_add.is_empty() {
            git_commit(&plan, &paths_to_add)?;
        }
    }

    // Step 8: Generate release notes BEFORE tagging (so git log range works)
    let notes = if !plan.no_gh_release {
        Some(generate_release_notes(&plan)?)
    } else {
        None
    };

    // Step 9: Tag and push
    git_tag_and_push(&plan, tag_exists)?;

    // Step 10: GitHub release
    if let Some(notes) = &notes {
        gh_release_create(&plan, &all_assets, notes)?;
    }

    // Step 11: Verify
    if !plan.no_gh_release {
        verify_release(&plan, &all_rpms)?;
    }

    // Step 12: Summary
    print_summary(ctx, &plan, &all_assets);

    Ok(())
}

// ---------------------------------------------------------------------------
// Phase A helpers
// ---------------------------------------------------------------------------

fn validate_flags(args: &GhReleaseArgs) -> Result<()> {
    if args.no_stage && args.no_gh_release {
        bail!(
            "cannot use --no-stage and --no-gh-release together\n\
             that would reduce to a plain build, which is already `cargo tizen rpm`"
        );
    }
    Ok(())
}

fn resolve_config(ctx: &AppContext, args: &GhReleaseArgs) -> ResolvedConfig {
    let cfg = &ctx.config.gh_release;

    let arches = if !args.arch.is_empty() {
        args.arch.clone()
    } else if let Some(ref configured) = cfg.arches {
        configured.iter().filter_map(|s| Arch::parse(s)).collect()
    } else {
        vec![Arch::Armv7l, Arch::Aarch64]
    };

    let remote = args
        .remote
        .clone()
        .or_else(|| cfg.remote.clone())
        .unwrap_or_else(|| "origin".to_string());

    let branch = args
        .branch
        .clone()
        .or_else(|| cfg.branch.clone())
        .unwrap_or_else(|| "main".to_string());

    let tag_format = args
        .tag_format
        .clone()
        .or_else(|| cfg.tag_format.clone())
        .unwrap_or_else(|| "v{version}".to_string());

    let sync_spec_version = cfg.sync_spec_version.unwrap_or(false);

    let package = args
        .package
        .clone()
        .or_else(|| cfg.package.clone())
        .or_else(|| ctx.config.primary_package().map(String::from));

    ResolvedConfig {
        package,
        arches,
        remote,
        branch,
        tag_format,
        sync_spec_version,
    }
}

fn preflight_checks(remote: &str, expected_branch: &str) -> Result<()> {
    // Check git is available
    which::which("git").context("git not found in PATH")?;

    // Check gh is available
    which::which("gh").context(
        "gh not found in PATH\n\
         install from: https://cli.github.com",
    )?;

    // Clean worktree
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

    // Correct branch
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .context("failed to determine current branch")?;
    let current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if current_branch != expected_branch {
        bail!(
            "releases must be created from branch {} (current: {})",
            expected_branch,
            if current_branch.is_empty() {
                "detached HEAD"
            } else {
                &current_branch
            }
        );
    }

    // gh auth
    let status = Command::new("gh")
        .args(["auth", "status"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to check gh auth status")?;
    if !status.success() {
        bail!("gh is not authenticated — run: gh auth login");
    }

    // Remote exists
    let status = Command::new("git")
        .args(["remote", "get-url", remote])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to check git remote")?;
    if !status.success() {
        bail!("git remote not found: {}", remote);
    }

    Ok(())
}

fn resolve_package_name(
    ctx: &AppContext,
    args: &GhReleaseArgs,
    resolved: &ResolvedConfig,
) -> Result<String> {
    if let Some(ref pkg) = resolved.package {
        return Ok(pkg.clone());
    }
    // Fall back to reading [package].name from Cargo.toml
    let name = default_package_name(&ctx.workspace_root);
    if name.is_empty() {
        bail!(
            "could not determine package name\n\
             use -p <name> or set [gh_release].package in .cargo-tizen.toml"
        );
    }
    let _ = args; // suppress unused warning
    Ok(name)
}

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
    let raw = fs::read_to_string(&toml_path)
        .with_context(|| format!("failed to read {}", toml_path.display()))?;
    let parsed: toml_types::CargoToml = basic_toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", toml_path.display()))?;

    // Try [package].version first
    if let Some(ref pkg) = parsed.package {
        if let Some(ref v) = pkg.version {
            return Ok((toml_path, v.clone()));
        }
    }
    // Then [workspace.package].version
    if let Some(ref ws) = parsed.workspace {
        if let Some(ref pkg) = ws.package {
            if let Some(ref v) = pkg.version {
                return Ok((toml_path, v.clone()));
            }
        }
    }

    // Try member directory: <workspace_root>/<package_name>/Cargo.toml
    let member_toml = workspace_root.join(package_name).join("Cargo.toml");
    if member_toml.is_file() {
        let raw = fs::read_to_string(&member_toml)
            .with_context(|| format!("failed to read {}", member_toml.display()))?;
        let parsed: toml_types::CargoToml = basic_toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", member_toml.display()))?;
        if let Some(ref pkg) = parsed.package {
            if let Some(ref v) = pkg.version {
                return Ok((member_toml, v.clone()));
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

fn read_cargo_version(workspace_root: &Path, package_name: &str) -> Result<String> {
    resolve_cargo_version_path(workspace_root, package_name).map(|(_, v)| v)
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

    let (m, n, p) = match level {
        BumpLevel::Major => (major + 1, 0, 0),
        BumpLevel::Minor => (major, minor + 1, 0),
        BumpLevel::Patch => (major, minor, patch + 1),
    };

    Ok(format!("{}.{}.{}", m, n, p))
}

fn update_cargo_toml_version(toml_path: &Path, old_version: &str, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(toml_path)
        .with_context(|| format!("failed to read {}", toml_path.display()))?;

    let mut lines: Vec<String> = Vec::new();
    let mut in_target_section = false;
    let mut updated = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track section headers
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
    let mut lines: Vec<String> = Vec::new();
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

fn format_tag(format: &str, version: &str) -> String {
    format.replace("{version}", version)
}

fn check_tag_exists(tag: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/tags/{}", tag)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn print_plan(plan: &ReleasePlan, spec_synced: bool, tag_exists: bool) {
    let use_color = color_enabled();
    let packaging_rel = plan
        .packaging_root
        .strip_prefix(&plan.workspace_root)
        .unwrap_or(&plan.packaging_root);

    println!();
    println!(
        "{} {} {}",
        cargo_status(use_color, "gh-release"),
        plan.package_name,
        plan.tag
    );
    if plan.version_bumped {
        println!("     Bump:    version -> {}", plan.version);
    }
    let arch_list: Vec<&str> = plan.arches.iter().map(|a| a.as_str()).collect();
    println!("     Build:   {} (release)", arch_list.join(", "));
    for arch in &plan.arches {
        println!(
            "     RPM:     {}-{}-1.{}.rpm",
            plan.package_name,
            plan.version,
            arch.rpm_arch()
        );
    }
    if !plan.no_stage {
        for arch in &plan.arches {
            println!(
                "     Stage:   {}/rpm/sources/{}-{}-1.{}.rpm",
                packaging_rel.display(),
                plan.package_name,
                plan.version,
                arch.rpm_arch()
            );
        }
    }
    if spec_synced {
        println!("     Spec:    Version: updated to {}", plan.version);
    }
    if !plan.no_stage {
        println!(
            "     Commit:  \"Update release artifacts for {}\"",
            plan.tag
        );
    }
    println!(
        "     Tag:     {} ({})",
        plan.tag,
        if tag_exists { "force-move" } else { "new" }
    );
    println!(
        "     Push:    {}/{} + tag {}",
        plan.remote, "main", plan.tag
    );
    if !plan.no_gh_release {
        let asset_count = plan.arches.len() * 2; // RPMs + SHA256
        println!(
            "     Release: GitHub release {} with {} assets{}",
            plan.tag,
            asset_count,
            if plan.draft { " (draft)" } else { "" }
        );
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

// ---------------------------------------------------------------------------
// Phase B helpers
// ---------------------------------------------------------------------------

fn self_exe() -> Result<PathBuf> {
    std::env::current_exe().context("failed to determine cargo-tizen binary path")
}

fn run_cargo_tizen(exe: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new(exe)
        .args(args)
        .status()
        .with_context(|| format!("failed to run: cargo tizen {}", args.join(" ")))?;
    if !status.success() {
        bail!("cargo tizen {} failed", args.join(" "));
    }
    Ok(())
}

fn stage_rpms(ctx: &AppContext, plan: &ReleasePlan, rpms: &[PathBuf]) -> Result<()> {
    let use_color = color_enabled();
    let dest_dir = plan.packaging_root.join("rpm").join("sources");

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

    // RPM filenames follow <name>-<version>-<release>.<arch>.rpm.
    // Filter to only include RPMs whose filename contains the target version
    // so stale RPMs from previous builds are not picked up.
    let version_needle = format!("-{}-", version);

    let mut rpms = Vec::new();
    for entry in fs::read_dir(&rpm_dir)
        .with_context(|| format!("failed to read RPM directory {}", rpm_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rpm") {
            let fname = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            if fname.contains(&version_needle) {
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

fn generate_release_notes(plan: &ReleasePlan) -> Result<String> {
    // --notes-file takes priority
    if let Some(ref path) = plan.notes_file {
        return fs::read_to_string(path)
            .with_context(|| format!("failed to read notes file {}", path.display()));
    }

    // notes_command from config
    if let Some(ref cmd) = plan.notes_command {
        if !cmd.is_empty() {
            let output = Command::new("sh")
                .args(["-c", cmd])
                .env("TAG", &plan.tag)
                .env("VERSION", &plan.version)
                .env("PACKAGE", &plan.package_name)
                .current_dir(&plan.workspace_root)
                .output()
                .with_context(|| format!("failed to run notes command: {}", cmd))?;
            if !output.status.success() {
                bail!(
                    "notes command failed: {}\n{}",
                    cmd,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    // Default: git log between previous tag and HEAD
    let output = Command::new("git")
        .args(["log", "--pretty=- %s", &format!("{}..HEAD", plan.tag)])
        .output();

    // If the tag doesn't exist yet or git log fails, use all commits
    let log_text = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            // Fallback: recent commits
            let output = Command::new("git")
                .args(["log", "--pretty=- %s", "-20"])
                .output()
                .unwrap_or_else(|_| std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
    };

    let notes = if log_text.is_empty() {
        format!("Release {}", plan.tag)
    } else {
        log_text
    };

    Ok(notes)
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

    // Check if there's anything to commit
    let output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&plan.workspace_root)
        .status();
    if let Ok(s) = output {
        if s.success() {
            // Nothing staged, skip commit
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

fn git_tag_and_push(plan: &ReleasePlan, tag_exists: bool) -> Result<()> {
    // Create or force-move tag
    let tag_message = format!("Release {}", plan.tag);
    if tag_exists {
        let status = Command::new("git")
            .args(["tag", "-fa", &plan.tag, "-m", &tag_message])
            .status()
            .context("failed to create git tag")?;
        if !status.success() {
            bail!("git tag failed");
        }
    } else {
        let status = Command::new("git")
            .args(["tag", "-a", &plan.tag, "-m", &tag_message])
            .status()
            .context("failed to create git tag")?;
        if !status.success() {
            bail!("git tag failed");
        }
    }

    // Push branch
    let status = Command::new("git")
        .args(["push", &plan.remote, "HEAD"])
        .status()
        .context("failed to push branch")?;
    if !status.success() {
        bail!(
            "git push failed\n\
             the tag {} was created locally — you may need to push manually",
            plan.tag
        );
    }

    // Push tag (force if it existed)
    let tag_ref = format!("refs/tags/{}", plan.tag);
    let mut push_args = vec!["push", &plan.remote, &tag_ref];
    if tag_exists {
        push_args.push("--force");
    }
    let status = Command::new("git")
        .args(&push_args)
        .status()
        .context("failed to push tag")?;
    if !status.success() {
        bail!("git push tag {} failed", plan.tag);
    }

    Ok(())
}

fn gh_release_create(plan: &ReleasePlan, assets: &[PathBuf], notes: &str) -> Result<()> {
    // Write notes to a temp file
    let notes_file =
        std::env::temp_dir().join(format!("cargo-tizen-notes-{}.md", std::process::id()));
    fs::write(&notes_file, notes).context("failed to write temp notes file")?;

    // Check if release already exists
    let existing = Command::new("gh")
        .args(["release", "view", &plan.tag])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if existing {
        // Update existing release
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
            .status()
            .context("failed to run gh release edit")?;
        if !status.success() {
            let _ = fs::remove_file(&notes_file);
            bail!("gh release edit failed");
        }

        // Upload assets with clobber
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
            .status()
            .context("failed to run gh release upload")?;
        if !status.success() {
            let _ = fs::remove_file(&notes_file);
            bail!("gh release upload failed");
        }
    } else {
        // Create new release
        let mut create_args = vec![
            "release".to_string(),
            "create".to_string(),
            plan.tag.clone(),
            "--title".to_string(),
            plan.tag.clone(),
            "--notes-file".to_string(),
            notes_file.to_string_lossy().to_string(),
        ];
        if plan.draft {
            create_args.push("--draft".to_string());
        }
        for asset in assets {
            create_args.push(asset.to_string_lossy().to_string());
        }
        let status = Command::new("gh")
            .args(&create_args)
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

fn verify_release(plan: &ReleasePlan, rpms: &[PathBuf]) -> Result<()> {
    // Verify tag resolves to HEAD
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .context("failed to get HEAD")?;
    let head_commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

    let remote_tag = Command::new("git")
        .args([
            "ls-remote",
            &plan.remote,
            &format!("refs/tags/{}^{{}}", plan.tag),
        ])
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

    // Verify assets exist in release
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
        .output()
        .context("failed to check release assets")?;
    let release_assets = String::from_utf8_lossy(&output.stdout);

    for rpm in rpms {
        let name = rpm
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
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

    // Get release URL
    let url = Command::new("gh")
        .args([
            "release", "view", &plan.tag, "--json", "url", "--jq", ".url",
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    println!();
    ctx.info(format!(
        "{} {}",
        cargo_status(use_color, "Released"),
        plan.tag
    ));

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
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

// ---------------------------------------------------------------------------
// TOML types for Cargo.toml parsing
// ---------------------------------------------------------------------------

mod toml_types {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct CargoToml {
        pub package: Option<PackageSection>,
        pub workspace: Option<WorkspaceSection>,
    }

    #[derive(Debug, Deserialize)]
    pub struct PackageSection {
        pub name: Option<String>,
        pub version: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WorkspaceSection {
        pub package: Option<WorkspacePackage>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WorkspacePackage {
        pub version: Option<String>,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn read_cargo_version_from_package() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        let v = read_cargo_version(dir.path(), "test").unwrap();
        assert_eq!(v, "1.2.3");
    }

    #[test]
    fn read_cargo_version_from_workspace() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        let v = read_cargo_version(dir.path(), "a").unwrap();
        assert_eq!(v, "0.5.0");
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
        fs::write(&spec, "Name:    test\nVersion:        0.1.0\nRelease: 1\n").unwrap();
        let v = read_spec_version(&spec).unwrap();
        assert_eq!(v, Some("0.1.0".to_string()));
    }

    #[test]
    fn read_spec_version_missing() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name:    test\nRelease: 1\n").unwrap();
        let v = read_spec_version(&spec).unwrap();
        assert_eq!(v, None);
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
    fn validate_flags_both_no_errors() {
        let args = GhReleaseArgs {
            package: None,
            arch: vec![],
            bump: None,
            remote: None,
            tag_format: None,
            branch: None,
            force_tag: false,
            sync_spec_version: false,
            no_stage: true,
            no_gh_release: true,
            notes_file: None,
            draft: false,
            dry_run: false,
            yes: false,
        };
        assert!(validate_flags(&args).is_err());
    }

    #[test]
    fn validate_flags_ok() {
        let args = GhReleaseArgs {
            package: None,
            arch: vec![],
            bump: None,
            remote: None,
            tag_format: None,
            branch: None,
            force_tag: false,
            sync_spec_version: false,
            no_stage: true,
            no_gh_release: false,
            notes_file: None,
            draft: false,
            dry_run: false,
            yes: false,
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
        // SHA256 of "hello world"
        assert!(
            content.starts_with("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
        assert!(content.contains("test.rpm"));
    }

    #[test]
    fn sync_spec_version_updates_field() {
        let dir = tempfile::tempdir().unwrap();
        let spec = dir.path().join("test.spec");
        fs::write(&spec, "Name:    test\nVersion:        0.1.0\nRelease: 1\n").unwrap();
        sync_spec_version(&spec, "0.2.0").unwrap();
        let content = fs::read_to_string(&spec).unwrap();
        assert!(content.contains("Version:        0.2.0"));
        assert!(!content.contains("0.1.0"));
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
}
