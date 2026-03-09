use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::context::AppContext;
use crate::sdk::TizenSdk;

#[derive(Debug, Clone)]
pub struct ResolvedToolchain {
    pub linker: String,
    pub cc: String,
    pub cxx: String,
    pub ar: String,
}

#[derive(Debug, Clone, Default)]
pub struct ToolEnv {
    vars: Vec<(String, String)>,
}

impl ToolEnv {
    pub fn for_cargo_build(
        ctx: &AppContext,
        arch: Arch,
        rust_target: &str,
        sysroot_dir: &Path,
    ) -> Self {
        let toolchain = resolve_toolchain(ctx, arch);
        let target_key = rust_target.to_string();
        let target_key_underscore = rust_target.replace('-', "_");
        let env_key = rust_target.replace('-', "_").to_uppercase();
        let mut env = Self::default();

        let rustflags_value = format!("-Clink-arg=--sysroot={}", sysroot_dir.display());
        let cflags_value = format!(
            "--sysroot={} -U__glibc_has_include -D__glibc_has_include(x)=0",
            sysroot_dir.display()
        );
        let pkg_config_libdir = format!(
            "{}:{}:{}",
            sysroot_dir.join("usr/lib/pkgconfig").display(),
            sysroot_dir.join("usr/lib64/pkgconfig").display(),
            sysroot_dir.join("usr/share/pkgconfig").display()
        );

        env.set(
            format!("CARGO_TARGET_{}_LINKER", env_key),
            &toolchain.linker,
        );
        env.set(
            format!("CARGO_TARGET_{}_RUSTFLAGS", env_key),
            &rustflags_value,
        );
        env.set("PKG_CONFIG_ALLOW_CROSS", "1");
        env.set("PKG_CONFIG_SYSROOT_DIR", &sysroot_dir.display().to_string());
        env.set("PKG_CONFIG_LIBDIR", &pkg_config_libdir);
        env.set(format!("CC_{}", env_key), &toolchain.cc);
        env.set(format!("CXX_{}", env_key), &toolchain.cxx);
        env.set(format!("AR_{}", env_key), &toolchain.ar);
        env.set(format!("CC_{}", target_key), &toolchain.cc);
        env.set(format!("CC_{}", target_key_underscore), &toolchain.cc);
        env.set(format!("CXX_{}", target_key), &toolchain.cxx);
        env.set(format!("CXX_{}", target_key_underscore), &toolchain.cxx);
        env.set(format!("AR_{}", target_key), &toolchain.ar);
        env.set(format!("AR_{}", target_key_underscore), &toolchain.ar);
        env.set(format!("CFLAGS_{}", target_key), &cflags_value);
        env.set(format!("CFLAGS_{}", target_key_underscore), &cflags_value);
        env.set(format!("CXXFLAGS_{}", target_key), &cflags_value);
        env.set(format!("CXXFLAGS_{}", target_key_underscore), &cflags_value);
        env.set(format!("CPPFLAGS_{}", target_key), &cflags_value);
        env.set(
            format!("CPPFLAGS_{}", target_key_underscore),
            &cflags_value,
        );
        env.set("USER_CPP_OPTS", "-std=c++17");
        configure_openssl_env(&mut env, &env_key, sysroot_dir);

        if let Some(path) = build_augmented_path(ctx, &toolchain) {
            env.set("PATH", path.to_string_lossy().to_string());
        }

        env
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.push((key.into(), value.into()));
    }

    pub fn set_if_unset(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        if std::env::var_os(&key).is_none() {
            self.vars.push((key, value.into()));
        }
    }

    pub fn apply(&self, command: &mut Command) {
        for (key, value) in &self.vars {
            command.env(key, value);
        }
    }
}

#[derive(Debug, Clone)]
struct OpenSslLocation {
    lib_dir: PathBuf,
    include_dir: PathBuf,
    root_dir: PathBuf,
}

fn configure_openssl_env(env: &mut ToolEnv, env_key: &str, sysroot_dir: &Path) {
    let Some(openssl) = discover_openssl_fallback(sysroot_dir) else {
        return;
    };

    let lib_dir = openssl.lib_dir.display().to_string();
    let include_dir = openssl.include_dir.display().to_string();
    let root_dir = openssl.root_dir.display().to_string();

    env.set_if_unset("OPENSSL_LIB_DIR", &lib_dir);
    env.set_if_unset("OPENSSL_INCLUDE_DIR", &include_dir);
    env.set_if_unset("OPENSSL_DIR", &root_dir);
    env.set_if_unset("OPENSSL_NO_VENDOR", "1");
    env.set_if_unset("OPENSSL_NO_PKG_CONFIG", "1");

    env.set_if_unset(format!("{env_key}_OPENSSL_LIB_DIR"), &lib_dir);
    env.set_if_unset(format!("{env_key}_OPENSSL_INCLUDE_DIR"), &include_dir);
    env.set_if_unset(format!("{env_key}_OPENSSL_DIR"), &root_dir);
    env.set_if_unset(format!("{env_key}_OPENSSL_NO_VENDOR"), "1");
    env.set_if_unset(format!("{env_key}_OPENSSL_NO_PKG_CONFIG"), "1");
}

fn discover_openssl_fallback(sysroot_dir: &Path) -> Option<OpenSslLocation> {
    let include_dir = sysroot_dir.join("usr").join("include");
    if !include_dir.join("openssl").is_dir() {
        return None;
    }

    let has_openssl_pc = [
        sysroot_dir.join("usr/lib/pkgconfig/openssl.pc"),
        sysroot_dir.join("usr/lib64/pkgconfig/openssl.pc"),
    ]
    .iter()
    .any(|path| path.is_file());
    if has_openssl_pc {
        return None;
    }

    for lib_dir in [sysroot_dir.join("usr/lib"), sysroot_dir.join("usr/lib64")] {
        if has_library(&lib_dir, "libssl") && has_library(&lib_dir, "libcrypto") {
            return Some(OpenSslLocation {
                lib_dir,
                include_dir: include_dir.clone(),
                root_dir: sysroot_dir.join("usr"),
            });
        }
    }

    None
}

fn has_library(lib_dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(lib_dir) else {
        return false;
    };

    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with(prefix)
            && (name.ends_with(".a") || name.ends_with(".so") || name.contains(".so."))
    })
}

pub fn verify_c_compiler_sanity(cc: &str, sysroot_dir: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new(cc);
    cmd.arg("-E").arg("-x").arg("c").arg("-");
    if let Some(sysroot_dir) = sysroot_dir {
        cmd.arg(format!("--sysroot={}", sysroot_dir.display()));
    }
    cmd.stdin(Stdio::null());

    let output = cmd.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_trimmed = stderr.trim();
    if stderr_trimmed.contains("/root/.dibs/") && stderr_trimmed.contains("Permission denied") {
        bail!(
            "compiler `{}` is unusable: it references inaccessible include paths under `/root/.dibs/...`. \
configure [arch.<name>].linker/cc/cxx in `.cargo-tizen.toml` to a valid cross-toolchain.\nstderr:\n{}",
            cc,
            stderr_trimmed
        );
    }

    if let Some(sysroot_dir) = sysroot_dir {
        bail!(
            "compiler `{}` failed sanity check (`-E -x c - --sysroot={}`). stderr:\n{}",
            cc,
            sysroot_dir.display(),
            stderr_trimmed
        );
    }

    bail!(
        "compiler `{}` failed sanity check (`-E -x c -`). stderr:\n{}",
        cc,
        stderr_trimmed
    )
}

pub fn resolve_toolchain(ctx: &AppContext, arch: Arch) -> ResolvedToolchain {
    let configured_linker = ctx.config.linker_for(arch);
    let linker = resolve_binary(ctx, &configured_linker);

    let cc = ctx
        .config
        .cc_for(arch)
        .map(|v| resolve_binary(ctx, &v))
        .unwrap_or_else(|| linker.clone());
    let cxx = ctx
        .config
        .cxx_for(arch)
        .map(|v| resolve_binary(ctx, &v))
        .or_else(|| infer_cxx_from_linker(&linker))
        .unwrap_or_else(|| cc.clone());
    let ar = ctx
        .config
        .ar_for(arch)
        .map(|v| resolve_binary(ctx, &v))
        .or_else(|| infer_ar_from_linker(&linker))
        .unwrap_or_else(|| "ar".to_string());

    ResolvedToolchain {
        linker,
        cc,
        cxx,
        ar,
    }
}

pub fn find_tool_in_sdk(sdk: &TizenSdk, tool: &str) -> Option<PathBuf> {
    let tools_dir = sdk.tools_dir();

    let direct = tools_dir.join("bin").join(executable_name(tool));
    if direct.is_file() {
        return Some(direct);
    }

    let entries = std::fs::read_dir(&tools_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(tool) {
            continue;
        }

        let candidate = path.join("bin").join(executable_name(tool));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn resolve_binary(ctx: &AppContext, value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_string();
    }

    if has_path_separator(value) {
        let path = PathBuf::from(value);
        if path.exists() {
            return path.display().to_string();
        }
        return value.to_string();
    }

    if let Ok(found) = which::which(value) {
        return found.display().to_string();
    }

    if let Some(sdk) = TizenSdk::locate(ctx.config.sdk_root().as_deref()) {
        if let Some(found) = find_tool_in_sdk(&sdk, value) {
            return found.display().to_string();
        }
    }

    value.to_string()
}

fn infer_cxx_from_linker(linker: &str) -> Option<String> {
    if linker.ends_with("gcc") {
        return Some(linker.trim_end_matches("gcc").to_string() + "g++");
    }
    if linker.ends_with("clang") {
        return Some(linker.to_string() + "++");
    }
    None
}

fn infer_ar_from_linker(linker: &str) -> Option<String> {
    if linker.ends_with("gcc") {
        return Some(linker.trim_end_matches("gcc").to_string() + "ar");
    }
    if linker.ends_with("clang") {
        return Some(linker.trim_end_matches("clang").to_string() + "ar");
    }
    None
}

fn build_augmented_path(ctx: &AppContext, toolchain: &ResolvedToolchain) -> Option<OsString> {
    let mut dirs = Vec::<PathBuf>::new();

    if let Some(sdk) = TizenSdk::locate(ctx.config.sdk_root().as_deref()) {
        dirs.push(sdk.tools_dir());
        dirs.push(sdk.tools_dir().join("ide").join("bin"));
    }
    for tool in [
        &toolchain.linker,
        &toolchain.cc,
        &toolchain.cxx,
        &toolchain.ar,
    ] {
        if has_path_separator(tool) {
            let path = PathBuf::from(tool);
            if let Some(parent) = path.parent() {
                dirs.push(parent.to_path_buf());
            }
        }
    }

    let existing_paths =
        std::env::var_os("PATH").map(|v| std::env::split_paths(&v).collect::<Vec<_>>())?;
    dirs.extend(existing_paths);

    let mut seen = HashSet::<PathBuf>::new();
    let mut unique = Vec::<PathBuf>::new();
    for dir in dirs {
        if !dir.as_os_str().is_empty() && seen.insert(dir.clone()) {
            unique.push(dir);
        }
    }

    std::env::join_paths(unique).ok()
}

fn has_path_separator(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
}

fn executable_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

pub fn rpmbuild_env(ctx: &AppContext) -> ToolEnv {
    let mut env = ToolEnv::default();
    if let Some(joined) = sdk_augmented_path(ctx) {
        env.set("PATH", joined.to_string_lossy());
    }
    env
}

pub fn tizen_cli_env(ctx: &AppContext) -> ToolEnv {
    let mut env = ToolEnv::default();
    if let Some(joined) = sdk_augmented_path(ctx) {
        env.set("PATH", joined.to_string_lossy());
    }
    env
}

fn sdk_augmented_path(ctx: &AppContext) -> Option<OsString> {
    let sdk = TizenSdk::locate(ctx.config.sdk_root().as_deref())?;
    let mut paths = Vec::new();
    paths.push(sdk.tools_dir());
    paths.push(sdk.tools_dir().join("ide").join("bin"));
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).ok()
}

pub fn ensure_rust_target_installed(target: &str) -> Result<bool> {
    let output = std::process::Command::new("rustup")
        .arg("target")
        .arg("list")
        .arg("--installed")
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|line| line.trim() == target))
}
