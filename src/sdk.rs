use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkFlavor {
    Cli,
    Extension,
}

impl Display for SdkFlavor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SdkFlavor::Cli => f.write_str("cli"),
            SdkFlavor::Extension => f.write_str("extension"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TizenSdk {
    root: PathBuf,
    flavor: SdkFlavor,
}

impl TizenSdk {
    pub fn locate(override_root: Option<&Path>) -> Option<Self> {
        for candidate in candidate_roots(override_root) {
            if !candidate.exists() {
                continue;
            }
            let flavor = if candidate
                .to_string_lossy()
                .contains(".tizen-extension-platform")
            {
                SdkFlavor::Extension
            } else {
                SdkFlavor::Cli
            };
            return Some(Self {
                root: candidate,
                flavor,
            });
        }
        None
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn flavor(&self) -> SdkFlavor {
        self.flavor
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.root.join("tools")
    }

    pub fn platforms_dir(&self) -> PathBuf {
        self.root.join("platforms")
    }

    pub fn tizen_cli(&self) -> PathBuf {
        if cfg!(windows) {
            self.tools_dir().join("ide").join("bin").join("tizen.bat")
        } else {
            self.tools_dir().join("ide").join("bin").join("tizen")
        }
    }

    pub fn sdb(&self) -> PathBuf {
        if cfg!(windows) {
            self.tools_dir().join("sdb.exe")
        } else {
            self.tools_dir().join("sdb")
        }
    }

    pub fn package_manager_cli(&self) -> PathBuf {
        if cfg!(windows) {
            self.root
                .join("package-manager")
                .join("package-manager-cli.exe")
        } else {
            self.root
                .join("package-manager")
                .join("package-manager-cli.bin")
        }
    }
}

fn candidate_roots(override_root: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(root) = override_root {
        candidates.push(root.to_path_buf());
    }

    if let Ok(value) = std::env::var("TIZEN_SDK") {
        if !value.trim().is_empty() {
            candidates.push(PathBuf::from(value));
        }
    }

    if let Ok(sdb) = which::which("sdb") {
        if sdb
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("tools")
        {
            if let Some(parent) = sdb.parent().and_then(Path::parent) {
                candidates.push(parent.to_path_buf());
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join(".tizen-extension-platform")
                .join("server")
                .join("sdktools")
                .join("data"),
        );
        candidates.push(home.join("tizen-studio"));
    }

    #[cfg(windows)]
    {
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            candidates.push(PathBuf::from(&system_drive).join("tizen-studio"));
            candidates.push(
                PathBuf::from(&system_drive)
                    .join(".tizen-extension-platform")
                    .join("server")
                    .join("sdktools")
                    .join("data"),
            );
        }
    }

    dedupe(candidates)
}

fn dedupe(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.iter().any(|existing| existing == &path) {
            unique.push(path);
        }
    }
    unique
}
