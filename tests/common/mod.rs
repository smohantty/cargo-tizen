use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Detected SDK environment. Only constructed when a usable SDK is found.
pub struct SdkEnv {
    pub sdk_root: PathBuf,
    /// Architectures that have a rootstrap installed and a working cross-compiler.
    pub ready_arches: Vec<String>,
    pub has_rpmbuild: bool,
    #[allow(dead_code)]
    pub has_tizen_cli: bool,
}

/// Detect the Tizen SDK and available architectures.
/// Returns `None` when no usable SDK is found — callers should skip the test.
pub fn detect_sdk() -> Option<SdkEnv> {
    let home = dirs::home_dir()?;
    let sdk_root = home.join("tizen-studio");
    if !sdk_root.join("platforms").is_dir() {
        return None;
    }

    let mut ready_arches = Vec::new();

    // Check armv7l
    if has_rootstrap_for(&sdk_root, "device") && has_cross_compiler("arm-linux-gnueabi-gcc") {
        ready_arches.push("armv7l".to_string());
    }

    // Check aarch64
    if has_rootstrap_for(&sdk_root, "device64") && has_cross_compiler("aarch64-linux-gnu-gcc") {
        ready_arches.push("aarch64".to_string());
    }

    if ready_arches.is_empty() {
        return None;
    }

    let has_rpmbuild = which_exists("rpmbuild");

    let has_tizen_cli = sdk_root
        .join("tools")
        .join("ide")
        .join("bin")
        .join("tizen")
        .is_file();

    Some(SdkEnv {
        sdk_root,
        ready_arches,
        has_rpmbuild,
        has_tizen_cli,
    })
}

fn has_rootstrap_for(sdk_root: &Path, rootstrap_type: &str) -> bool {
    let platforms = sdk_root.join("platforms");
    let Ok(entries) = fs::read_dir(&platforms) else {
        return false;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Walk profile dirs (e.g. tizen-10.0/tizen/rootstraps/)
        let rootstraps_base = path.join("tizen").join("rootstraps");
        if rootstraps_base.is_dir() {
            if let Ok(rs_entries) = fs::read_dir(&rootstraps_base) {
                for rs_entry in rs_entries.flatten() {
                    let name = rs_entry.file_name();
                    let name = name.to_string_lossy();
                    if name.contains(rootstrap_type)
                        && name.ends_with(".core")
                        && rs_entry.path().is_dir()
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn has_cross_compiler(name: &str) -> bool {
    which_exists(name)
}

fn which_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a minimal Rust project suitable for cross-compilation.
pub fn scaffold_rust_project(dir: &Path, name: &str) {
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .expect("write Cargo.toml");

    fs::create_dir_all(dir.join("src")).expect("create src/");
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");
}

/// Write a .cargo-tizen.toml with the given arch.
pub fn write_cargo_tizen_config(dir: &Path, arch: &str, sdk_root: &Path) {
    fs::write(
        dir.join(".cargo-tizen.toml"),
        format!(
            "[default]\n\
             arch = \"{arch}\"\n\
             profile = \"mobile\"\n\
             platform_version = \"10.0\"\n\
             \n\
             [package]\n\
             name = \"test-app\"\n\
             packages = [\"test-app\"]\n\
             \n\
             [sdk]\n\
             root = \"{}\"\n",
            sdk_root.display()
        ),
    )
    .expect("write .cargo-tizen.toml");
}
