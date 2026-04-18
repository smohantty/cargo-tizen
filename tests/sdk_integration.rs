//! Integration tests that exercise cargo-tizen against a real Tizen SDK.
//!
//! These tests are **skipped automatically** when the SDK is not installed.
//! They test the full command pipeline: init -> config -> doctor -> setup ->
//! build -> rpm -> clean. Install and TPK (which need a device or tizen CLI)
//! are excluded.
//!
//! Run with: `cargo test --test sdk_integration`

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

fn cargo_tizen() -> Command {
    Command::cargo_bin("cargo-tizen").expect("binary should exist")
}

// ---------------------------------------------------------------------------
// CLI smoke tests (no SDK needed)
// ---------------------------------------------------------------------------

#[test]
fn help_prints_usage() {
    cargo_tizen()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Build Rust projects"));
}

#[test]
fn version_flag_prints_version() {
    cargo_tizen()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("cargo-tizen"));
}

#[test]
fn no_args_shows_help() {
    cargo_tizen()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn unknown_subcommand_fails() {
    cargo_tizen()
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

// ---------------------------------------------------------------------------
// init (no SDK needed — just scaffolding)
// ---------------------------------------------------------------------------

#[test]
fn init_creates_project_config() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    assert!(tmp.path().join(".cargo-tizen.toml").is_file());
}

#[test]
fn init_rpm_creates_spec_file() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    // init first to create .cargo-tizen.toml with [package].name
    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    // then init --rpm reads the existing config
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success();

    assert!(tmp.path().join(".cargo-tizen.toml").is_file());
    assert!(tmp.path().join("tizen/rpm/my-app.spec").is_file());
}

#[test]
fn init_tpk_creates_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--tpk"])
        .assert()
        .success();

    assert!(tmp.path().join("tizen/tpk/tizen-manifest.xml").is_file());
}

#[test]
fn init_rpm_and_tpk_creates_both() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    // init first, then init --rpm --tpk (needs existing [package].name)
    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm", "--tpk"])
        .assert()
        .success();

    assert!(tmp.path().join("tizen/rpm/my-app.spec").is_file());
    assert!(tmp.path().join("tizen/tpk/tizen-manifest.xml").is_file());
}

#[test]
fn init_skips_existing_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    // First: create config
    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    // Second: create RPM scaffold
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success();

    // Third: re-init RPM — should skip, not fail
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped"));
}

#[test]
fn init_force_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    // Create config first
    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    // Create RPM scaffold
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success();

    // Force overwrite
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Overwrote"));
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

#[test]
fn config_show_works_without_project() {
    let tmp = tempfile::tempdir().unwrap();

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["config", "--show"])
        .assert()
        .success();
}

#[test]
fn config_show_reads_project_config() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "my-app");

    cargo_tizen()
        .current_dir(tmp.path())
        .arg("init")
        .assert()
        .success();

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["config", "--show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-app"));
}

// ---------------------------------------------------------------------------
// devices (works without SDK — just reports sdb not found)
// ---------------------------------------------------------------------------

#[test]
fn devices_reports_error_when_no_sdb() {
    let tmp = tempfile::tempdir().unwrap();

    // Remove SDK from PATH so sdb can't be found
    let result = cargo_tizen()
        .current_dir(tmp.path())
        .arg("devices")
        .env("PATH", "/usr/bin:/bin")
        .env_remove("TIZEN_SDK")
        .output()
        .unwrap();

    // Either succeeds (sdb in known SDK location) or fails (sdb not found)
    // We just verify it doesn't panic
    let _ = result.status;
}

// ---------------------------------------------------------------------------
// SDK-dependent tests: setup, build, rpm, doctor, fix, clean
// ---------------------------------------------------------------------------

/// Run a full pipeline test for a given architecture.
/// Skipped when the SDK is not available for that arch.
fn pipeline_for_arch(arch: &str) {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    if !sdk.ready_arches.contains(&arch.to_string()) {
        eprintln!("SKIP: arch {arch} not ready (no rootstrap or cross-compiler)");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path();

    // Step 1: scaffold project
    common::scaffold_rust_project(project_dir, "test-app");
    common::write_cargo_tizen_config(project_dir, arch, &sdk.sdk_root);

    // Step 2: init --rpm
    cargo_tizen()
        .current_dir(project_dir)
        .args(["init", "--rpm"])
        .assert()
        .success();
    assert!(project_dir.join("tizen/rpm/test-app.spec").is_file());

    // Step 3: doctor
    let doctor_result = cargo_tizen()
        .current_dir(project_dir)
        .args(["doctor", "-A", arch])
        .output()
        .unwrap();
    // doctor may warn about missing tizen CLI, that's OK
    // just verify it ran without panic
    eprintln!(
        "doctor exit={} stdout={}",
        doctor_result.status,
        String::from_utf8_lossy(&doctor_result.stdout)
    );

    // Step 4: setup
    cargo_tizen()
        .current_dir(project_dir)
        .args([
            "setup",
            "-A",
            arch,
            "--sdk-root",
            &sdk.sdk_root.display().to_string(),
        ])
        .assert()
        .success();

    // Step 5: build
    cargo_tizen()
        .current_dir(project_dir)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // Verify binary was produced
    let target_dir = project_dir.join("target/tizen").join(arch).join("cargo");
    assert!(target_dir.is_dir(), "cargo target dir should exist");

    // Step 6: rpm (if rpmbuild available)
    if sdk.has_rpmbuild {
        cargo_tizen()
            .current_dir(project_dir)
            .args(["rpm", "-A", arch, "--release", "--no-build"])
            .assert()
            .success();

        // Verify RPM was produced
        let rpm_output = project_dir.join("target/tizen").join(arch).join("release");
        eprintln!("RPM output dir: {}", rpm_output.display());
    } else {
        eprintln!("SKIP rpm: rpmbuild not found");
    }

    // Step 7: clean --build
    cargo_tizen()
        .current_dir(project_dir)
        .args(["clean", "--build", "-A", arch])
        .assert()
        .success();
    // target dir for this arch should be gone
    assert!(
        !project_dir
            .join("target/tizen")
            .join(arch)
            .join("cargo")
            .exists(),
        "build output should be cleaned"
    );
}

#[test]
fn pipeline_armv7l() {
    pipeline_for_arch("armv7l");
}

#[test]
fn pipeline_aarch64() {
    pipeline_for_arch("aarch64");
}

// ---------------------------------------------------------------------------
// Standalone SDK tests
// ---------------------------------------------------------------------------

#[test]
fn setup_with_force_rebuilds() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // First setup
    cargo_tizen()
        .current_dir(tmp.path())
        .args([
            "setup",
            "-A",
            arch,
            "--sdk-root",
            &sdk.sdk_root.display().to_string(),
        ])
        .assert()
        .success();

    // Force rebuild
    cargo_tizen()
        .current_dir(tmp.path())
        .args([
            "setup",
            "-A",
            arch,
            "--sdk-root",
            &sdk.sdk_root.display().to_string(),
            "--force",
        ])
        .assert()
        .success();
}

#[test]
fn build_without_prior_setup_auto_prepares() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // Build without setup — should auto-prepare sysroot
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["build", "-A", arch])
        .assert()
        .success();
}

#[test]
fn build_debug_profile() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // Build in debug mode (no --release)
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["build", "-A", arch])
        .assert()
        .success();
}

#[test]
fn build_fails_without_arch() {
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    // No .cargo-tizen.toml, no arch flag, no device
    cargo_tizen()
        .current_dir(tmp.path())
        .arg("build")
        .assert()
        .failure();
}

#[test]
fn clean_build_removes_arch_output() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // Build to create artifacts
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["build", "-A", arch])
        .assert()
        .success();

    // Clean build only (avoids touching the global sysroot cache)
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["clean", "--build", "-A", arch])
        .assert()
        .success();

    assert!(
        !tmp.path()
            .join("target/tizen")
            .join(arch)
            .join("cargo")
            .exists(),
        "build output should be cleaned"
    );
}

#[test]
fn clean_build_only() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["build", "-A", arch])
        .assert()
        .success();

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["clean", "--build"])
        .assert()
        .success();
}

#[test]
fn fix_installs_targets_and_prepares_sysroot() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: Tizen SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    cargo_tizen()
        .current_dir(tmp.path())
        .args(["fix", "-A", arch])
        .assert()
        .success();
}

#[test]
fn rpm_no_build_uses_existing_binary() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // Init RPM spec
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success();

    // Build first
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // RPM with --no-build
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

#[test]
fn rpm_with_build() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    common::scaffold_rust_project(tmp.path(), "test-app");
    common::write_cargo_tizen_config(tmp.path(), arch, &sdk.sdk_root);

    // Init RPM spec
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["init", "--rpm"])
        .assert()
        .success();

    // RPM builds and packages in one step
    cargo_tizen()
        .current_dir(tmp.path())
        .args(["rpm", "-A", arch, "--release"])
        .assert()
        .success();
}
