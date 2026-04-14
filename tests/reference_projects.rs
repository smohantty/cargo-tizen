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
use std::path::Path;

fn cargo_tizen() -> Command {
    Command::cargo_bin("cargo-tizen").expect("binary should exist")
}

fn require_sdk() -> Option<common::SdkEnv> {
    match common::detect_sdk() {
        Some(sdk) => Some(sdk),
        None => {
            eprintln!("SKIP: SDK not found");
            None
        }
    }
}

fn require_sdk_with_rpmbuild() -> Option<common::SdkEnv> {
    match common::detect_sdk() {
        Some(sdk) if sdk.has_rpmbuild => Some(sdk),
        _ => {
            eprintln!("SKIP: SDK or rpmbuild not found");
            None
        }
    }
}

fn with_project_for_each_arch(
    sdk: &common::SdkEnv,
    project_name: &str,
    mut f: impl FnMut(&Path, &str),
) {
    for arch in &sdk.ready_arches {
        let tmp = tempfile::tempdir().unwrap();
        let project = common::copy_reference_project(project_name, tmp.path());
        common::patch_sdk_root(&project, &sdk.sdk_root);
        f(&project, arch);
    }
}

fn build_release(project: &Path, arch: &str) {
    cargo_tizen()
        .current_dir(project)
        .args(["build", "-A", arch, "--release"])
        .assert()
        .success();
}

fn package_rpm(project: &Path, arch: &str) {
    cargo_tizen()
        .current_dir(project)
        .args(["rpm", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

fn package_tpk(project: &Path, arch: &str) {
    cargo_tizen()
        .current_dir(project)
        .args(["tpk", "-A", arch, "--release", "--no-build"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// rpm-app: single binary, minimal RPM
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_build_and_package() {
    let Some(sdk) = require_sdk_with_rpmbuild() else {
        return;
    };

    with_project_for_each_arch(&sdk, "rpm-app", |project, arch| {
        build_release(project, arch);
        package_rpm(project, arch);
    });
}

fn rpm_project_build_and_package(project_name: &str) {
    let Some(sdk) = require_sdk_with_rpmbuild() else {
        return;
    };

    with_project_for_each_arch(&sdk, project_name, |project, arch| {
        build_release(project, arch);
        package_rpm(project, arch);
    });
}

// ---------------------------------------------------------------------------
// rpm-service-app: single binary with extra sources (systemd unit + env)
// ---------------------------------------------------------------------------

#[test]
fn rpm_service_app_build_and_package() {
    rpm_project_build_and_package("rpm-service-app");
}

// ---------------------------------------------------------------------------
// rpm-multi-package: workspace with two binaries in one RPM
// ---------------------------------------------------------------------------

#[test]
fn rpm_multi_package_build_and_package() {
    rpm_project_build_and_package("rpm-multi-package");
}

// ---------------------------------------------------------------------------
// rpm-syslibs: binary linking openssl + sqlite from sysroot
//
// ---------------------------------------------------------------------------

#[test]
fn rpm_syslibs_build_and_package() {
    rpm_project_build_and_package("rpm-syslibs");
}

// ---------------------------------------------------------------------------
// tpk-service-app: TPK packaging (build only — needs tizen CLI for packaging)
// ---------------------------------------------------------------------------

#[test]
fn tpk_service_app_build() {
    let Some(sdk) = require_sdk() else {
        return;
    };

    with_project_for_each_arch(&sdk, "tpk-service-app", |project, arch| {
        build_release(project, arch);

        if sdk.has_tizen_cli {
            package_tpk(project, arch);
        } else {
            eprintln!("SKIP tpk packaging for {arch}: tizen CLI not found");
        }
    });
}

// ---------------------------------------------------------------------------
// Cross-arch: build each reference project for all available architectures
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_builds_for_all_arches() {
    let Some(sdk) = require_sdk() else {
        return;
    };

    with_project_for_each_arch(&sdk, "rpm-app", build_release);
}

// ---------------------------------------------------------------------------
// Clean after build: verify clean removes outputs without errors
// ---------------------------------------------------------------------------

#[test]
fn rpm_app_clean_after_build() {
    let Some(sdk) = require_sdk() else {
        return;
    };

    with_project_for_each_arch(&sdk, "rpm-app", |project, arch| {
        build_release(project, arch);

        assert!(project.join("target/tizen").join(arch).exists());

        cargo_tizen()
            .current_dir(project)
            .args(["clean", "--build", "-A", arch])
            .assert()
            .success();

        assert!(
            !project
                .join("target/tizen")
                .join(arch)
                .join("cargo")
                .exists()
        );
    });
}

// ---------------------------------------------------------------------------
// Doctor: run doctor on each reference project
// ---------------------------------------------------------------------------

#[test]
fn doctor_on_reference_projects() {
    let Some(sdk) = require_sdk() else {
        return;
    };

    for project_name in &[
        "rpm-app",
        "rpm-service-app",
        "rpm-syslibs",
        "rpm-multi-package",
        "tpk-service-app",
    ] {
        with_project_for_each_arch(&sdk, project_name, |project, arch| {
            let result = cargo_tizen()
                .current_dir(project)
                .args(["doctor", "-A", arch])
                .output()
                .unwrap();

            eprintln!(
                "doctor {} arch={} exit={} stdout={}",
                project_name,
                arch,
                result.status,
                String::from_utf8_lossy(&result.stdout)
            );
        });
    }
}
