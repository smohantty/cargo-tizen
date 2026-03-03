use std::fmt::{Display, Formatter};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    Armv7l,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchMap {
    pub rust_target: &'static str,
    pub tizen_cli_arch: &'static str,
    pub tizen_build_arch: &'static str,
    pub rpm_build_arch: &'static str,
    pub rootstrap_type: &'static str,
    pub default_linker: &'static str,
}

impl Arch {
    pub fn map(self) -> ArchMap {
        match self {
            Arch::Armv7l => ArchMap {
                rust_target: "armv7-unknown-linux-gnueabihf",
                tizen_cli_arch: "arm",
                tizen_build_arch: "armel",
                rpm_build_arch: "armv7l",
                rootstrap_type: "device",
                default_linker: "arm-linux-gnueabi-gcc",
            },
            Arch::Aarch64 => ArchMap {
                rust_target: "aarch64-unknown-linux-gnu",
                tizen_cli_arch: "aarch64",
                tizen_build_arch: "aarch64",
                rpm_build_arch: "aarch64",
                rootstrap_type: "device64",
                default_linker: "aarch64-linux-gnu-gcc",
            },
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Arch::Armv7l => "armv7l",
            Arch::Aarch64 => "aarch64",
        }
    }

    pub fn rust_target(self) -> &'static str {
        self.map().rust_target
    }

    pub fn rpm_arch(self) -> &'static str {
        self.map().rpm_build_arch
    }

    pub fn tizen_cli_arch(self) -> &'static str {
        self.map().tizen_cli_arch
    }

    pub fn tizen_build_arch(self) -> &'static str {
        self.map().tizen_build_arch
    }

    pub fn rootstrap_type(self) -> &'static str {
        self.map().rootstrap_type
    }

    pub fn default_linker(self) -> &'static str {
        self.map().default_linker
    }
}

impl Display for Arch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::Arch;

    #[test]
    fn mapping_is_stable() {
        assert_eq!(Arch::Armv7l.rust_target(), "armv7-unknown-linux-gnueabihf");
        assert_eq!(Arch::Aarch64.rust_target(), "aarch64-unknown-linux-gnu");
        assert_eq!(Arch::Armv7l.tizen_cli_arch(), "arm");
        assert_eq!(Arch::Armv7l.tizen_build_arch(), "armel");
        assert_eq!(Arch::Armv7l.rpm_arch(), "armv7l");
    }
}
