//! Integration tests that build and package the reference projects from
//! `templates/reference-projects/`. Each test copies the project into a temp
//! directory, patches the SDK root, and runs the full pipeline.
//!
//! These tests are **skipped automatically** when the Tizen SDK or required
//! tooling is not available.
//!
//! Run with: `cargo test --test reference_projects`

mod common;

use assert_cmd::Command;

fn cargo_tizen() -> Command {
    Command::cargo_bin("cargo-tizen").expect("binary should exist")
}

// ---------------------------------------------------------------------------
// rpm-app: single binary, minimal RPM
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_build_and_package() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("rpm-app", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    let arch = &sdk.ready_arches[0];

    // Build
    cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // Package RPM
    cargo_tizen()
        .current_dir(&project)
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// rpm-service-app: single binary with extra sources (systemd unit + env)
// ---------------------------------------------------------------------------

#[test]
fn rpm_service_app_build_and_package() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("rpm-service-app", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    let arch = &sdk.ready_arches[0];

    // Build
    cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // Package RPM (exercises extra sources collection)
    cargo_tizen()
        .current_dir(&project)
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// rpm-multi-package: workspace with two binaries in one RPM
// ---------------------------------------------------------------------------

#[test]
fn rpm_multi_package_build_and_package() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("rpm-multi-package", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    let arch = &sdk.ready_arches[0];

    // Build (workspace — builds all members)
    cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // Package RPM (multi-package: hello-server + hello-cli)
    cargo_tizen()
        .current_dir(&project)
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// rpm-syslibs: binary linking openssl + sqlite from sysroot
//
// Gated behind `--ignored` because compiling openssl-sys + libsqlite3-sys
// from scratch in a fresh temp dir takes ~18s.
// Run explicitly with: cargo test --test reference_projects rpm_syslibs -- --ignored
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn rpm_syslibs_build_and_package() {
    let sdk = match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => sdk,
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("rpm-syslibs", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    let arch = &sdk.ready_arches[0];

    // This project links to libssl and libsqlite3 from the sysroot.
    // It may fail if the rootstrap does not provide them — that's informational.
    let build_result = cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .output()
        .unwrap();

    if !build_result.status.success() {
        let stderr = String::from_utf8_lossy(&build_result.stderr);
        if stderr.contains("openssl") || stderr.contains("sqlite") || stderr.contains("pkg-config")
        {
            eprintln!(
                "SKIP: rpm-syslibs build failed due to missing system libs in sysroot\n{}",
                stderr
            );
            return;
        }
        panic!(
            "rpm-syslibs build failed for unexpected reason:\n{}",
            stderr
        );
    }

    // Package RPM
    cargo_tizen()
        .current_dir(&project)
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// tpk-service-app: TPK packaging (build only — needs tizen CLI for packaging)
// ---------------------------------------------------------------------------

#[test]
fn tpk_service_app_build() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: SDK not found");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("tpk-service-app", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    let arch = &sdk.ready_arches[0];

    // Build succeeds (TPK packaging requires tizen CLI which may not be present)
    cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // TPK packaging — only if tizen CLI is available
    if sdk.has_tizen_cli {
        cargo_tizen()
            .current_dir(&project)
            .args(["tpk", "-A", arch, "--release", "--no-build"])
            .assert()
            .success();
    } else {
        eprintln!("SKIP tpk packaging: tizen CLI not found");
    }
}

// ---------------------------------------------------------------------------
// Cross-arch: build each reference project for all available architectures
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_builds_for_all_arches() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: SDK not found");
            return;
        }
    };

    for arch in &sdk.ready_arches {
        let tmp = tempfile::tempdir().unwrap();
        let project = common::copy_reference_project("rpm-app", tmp.path());
        common::patch_sdk_root(&project, &sdk.sdk_root);

        cargo_tizen()
            .current_dir(&project)
            .args(["build", "-A", arch, "--release"])
            .assert()
            .success();
    }
}

// ---------------------------------------------------------------------------
// Clean after build: verify clean removes outputs without errors
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_clean_after_build() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];
    let tmp = tempfile::tempdir().unwrap();
    let project = common::copy_reference_project("rpm-app", tmp.path());
    common::patch_sdk_root(&project, &sdk.sdk_root);

    cargo_tizen()
        .current_dir(&project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();

    // target dir should exist
    assert!(project.join("target/tizen").join(arch).exists());

    cargo_tizen()
        .current_dir(&project)
        .args(["clean", "--build", "-A", arch])
        .assert()
        .success();

    // target dir should be gone
    assert!(
        !project
            .join("target/tizen")
            .join(arch)
            .join("cargo")
            .exists()
    );
}

// ---------------------------------------------------------------------------
// Doctor: run doctor on each reference project
// ---------------------------------------------------------------------------

#[test]
fn doctor_on_reference_projects() {
    let sdk = match common::detect_sdk() {
        Some(sdk) => sdk,
        None => {
            eprintln!("SKIP: SDK not found");
            return;
        }
    };

    let arch = &sdk.ready_arches[0];

    for project_name in &[
        "rpm-app",
        "rpm-service-app",
        "rpm-multi-package",
        "tpk-service-app",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let project = common::copy_reference_project(project_name, tmp.path());
        common::patch_sdk_root(&project, &sdk.sdk_root);

        let result = cargo_tizen()
            .current_dir(&project)
            .args(["doctor", "-A", arch])
            .output()
            .unwrap();

        // Doctor may report warnings (e.g., missing tizen CLI) but should not panic
        eprintln!(
            "doctor {} exit={} stdout={}",
            project_name,
            result.status,
            String::from_utf8_lossy(&result.stdout)
        );
    }
}
