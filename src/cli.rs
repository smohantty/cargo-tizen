use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::arch::Arch;
use crate::sysroot::provider::ProviderKind;

const ROOT_AFTER_HELP: &str = "\
Quick start:
  cargo tizen doctor
  cargo tizen fix

Common workflows:
  cargo tizen build -A armv7l --release
  cargo tizen rpm -A armv7l --cargo-release
  cargo tizen tpk -A armv7l --cargo-release
  cargo tizen install -A armv7l --cargo-release

Tips:
  Most commands auto-select the target architecture when exactly one choice is available.
  Use cargo tizen <command> --help for command-specific notes and examples.";

const SETUP_AFTER_HELP: &str = "\
Examples:
  cargo tizen setup -A armv7l --profile mobile --platform-version 10.0
  cargo tizen setup -A aarch64 --sdk-root /opt/tizen-studio

Notes:
  setup is optional for normal build and packaging flows.
  build, rpm, tpk, and install prepare the sysroot automatically when needed.";

const BUILD_AFTER_HELP: &str = "\
Examples:
  cargo tizen build -A armv7l
  cargo tizen build -A aarch64 --release
  cargo tizen build -A armv7l -- --features my_feature

Notes:
  Pass extra Cargo arguments after -- so they are forwarded to cargo build unchanged.
  When -A is omitted, cargo-tizen tries config and connected-device auto-detection first.";

const RPM_AFTER_HELP: &str = "\
Examples:
  cargo tizen rpm -A armv7l --cargo-release
  cargo tizen rpm -A aarch64 --cargo-release --packaging-dir ./packaging
  cargo tizen rpm -p my-server --no-build

Notes:
  The RPM spec must already exist at <packaging-dir>/rpm/<package-name>.spec.
  Use -p or [default].package when packaging a workspace member.";

const TPK_AFTER_HELP: &str = "\
Examples:
  cargo tizen tpk -A armv7l --cargo-release
  cargo tizen tpk -A aarch64 --no-build --packaging-dir ./packaging
  cargo tizen tpk -A armv7l --sign my_profile

Notes:
  The TPK manifest must already exist at <packaging-dir>/tpk/tizen-manifest.xml.
  Use --sign to pick a Tizen Studio certificate profile for signing.";

const DEVICES_AFTER_HELP: &str = "\
Examples:
  cargo tizen devices
  cargo tizen devices --all

Notes:
  By default, output focuses on ready Tizen devices.
  Use --all to include offline, unauthorized, and non-Tizen entries parsed from sdb.";

const INSTALL_AFTER_HELP: &str = "\
Examples:
  cargo tizen install -A armv7l --cargo-release
  cargo tizen install -A aarch64 -d 192.168.0.101:26101 --cargo-release
  cargo tizen install --tpk ./build/app.tpk -d <device-id>

Notes:
  install is TPK-only.
  If --tpk is omitted, cargo-tizen builds and packages a TPK before installing it.";

const DOCTOR_AFTER_HELP: &str = "\
Examples:
  cargo tizen doctor
  cargo tizen doctor -A armv7l
  cargo tizen -v doctor

Notes:
  doctor checks both supported architectures unless -A is passed.
  Use -v for per-check path details and extra diagnostics.";

const FIX_AFTER_HELP: &str = "\
Examples:
  cargo tizen fix
  cargo tizen fix -A armv7l

Notes:
  fix installs missing Rust targets with rustup and prepares missing sysroots.
  Host package manager dependencies such as rpmbuild are reported but not installed automatically.";

const CLEAN_AFTER_HELP: &str = "\
Examples:
  cargo tizen clean --build
  cargo tizen clean --sysroot -A aarch64
  cargo tizen clean --all

Notes:
  --build removes generated build and packaging outputs.
  --sysroot removes cached sysroots so they are rebuilt on the next command.";

const CONFIG_AFTER_HELP: &str = "\
Examples:
  cargo tizen config --show
  cargo tizen config --sign my_profile
  cargo tizen config --sign \"\"

Notes:
  Persistent user settings live in ~/.config/cargo-tizen/config.toml.
  Command-line flags still override stored defaults for the current invocation.";

#[derive(Debug, Parser)]
#[command(
    name = "cargo-tizen",
    bin_name = "cargo tizen",
    about = "Cross-build Rust projects for Tizen and package them as RPM or TPK",
    long_about = "Cross-build Rust projects for Tizen, prepare SDK sysroots, and package artifacts as RPM or TPK.\n\nStart with doctor to verify prerequisites, use fix to repair common setup gaps, then use build, rpm, tpk, or install for day-to-day work.",
    after_help = ROOT_AFTER_HELP,
    after_long_help = ROOT_AFTER_HELP,
    arg_required_else_help = true,
    propagate_version = true,
    version
)]
pub struct Cli {
    #[arg(
        short,
        long,
        global = true,
        help = "Print detailed progress and diagnostic output"
    )]
    pub verbose: bool,

    #[arg(
        short,
        long,
        global = true,
        help = "Reduce output to warnings and errors"
    )]
    pub quiet: bool,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Load an additional config file after the default user and project config files"
    )]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(
        about = "Prepare and cache a Tizen sysroot for cross-compilation",
        after_help = SETUP_AFTER_HELP,
        after_long_help = SETUP_AFTER_HELP
    )]
    Setup(SetupArgs),
    #[command(
        about = "Cross-build the current Rust project for a Tizen target",
        after_help = BUILD_AFTER_HELP,
        after_long_help = BUILD_AFTER_HELP
    )]
    Build(BuildArgs),
    #[command(
        about = "Package the project as an RPM using an existing spec file",
        after_help = RPM_AFTER_HELP,
        after_long_help = RPM_AFTER_HELP
    )]
    Rpm(RpmArgs),
    #[command(
        about = "Package the project as a signed TPK using the Tizen CLI",
        after_help = TPK_AFTER_HELP,
        after_long_help = TPK_AFTER_HELP
    )]
    Tpk(TpkArgs),
    #[command(
        about = "List connected Tizen devices discovered via sdb",
        after_help = DEVICES_AFTER_HELP,
        after_long_help = DEVICES_AFTER_HELP
    )]
    Devices(DevicesArgs),
    #[command(
        about = "Build or reuse a TPK and install it on a connected device",
        after_help = INSTALL_AFTER_HELP,
        after_long_help = INSTALL_AFTER_HELP
    )]
    Install(InstallArgs),
    #[command(
        about = "Check SDK, toolchain, sysroot, and packaging readiness",
        after_help = DOCTOR_AFTER_HELP,
        after_long_help = DOCTOR_AFTER_HELP
    )]
    Doctor(DoctorArgs),
    #[command(
        about = "Install missing Rust targets and prepare missing sysroots",
        after_help = FIX_AFTER_HELP,
        after_long_help = FIX_AFTER_HELP
    )]
    Fix(FixArgs),
    #[command(
        about = "Remove build outputs and/or cached sysroots",
        after_help = CLEAN_AFTER_HELP,
        after_long_help = CLEAN_AFTER_HELP
    )]
    Clean(CleanArgs),
    #[command(
        about = "View or update persistent cargo-tizen settings",
        after_help = CONFIG_AFTER_HELP,
        after_long_help = CONFIG_AFTER_HELP
    )]
    Config(ConfigArgs),
}

#[derive(Debug, Clone, Args)]
pub struct SetupArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to prepare (auto-detected when omitted)"
    )]
    pub arch: Option<Arch>,

    #[arg(
        long,
        help = "Tizen profile to resolve, such as mobile or tv, when selecting a rootstrap"
    )]
    pub profile: Option<String>,

    #[arg(long, help = "Tizen platform version to resolve, such as 10.0")]
    pub platform_version: Option<String>,

    #[arg(
        long,
        value_enum,
        help = "Sysroot source to use when preparing the cache"
    )]
    pub provider: Option<ProviderKind>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Override the detected Tizen Studio root for this run"
    )]
    pub sdk_root: Option<PathBuf>,

    #[arg(
        long,
        help = "Rebuild the cached sysroot even if a matching entry already exists"
    )]
    pub force: bool,
}

#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to build for (auto-detected when omitted)"
    )]
    pub arch: Option<Arch>,

    #[arg(long, help = "Build in release mode")]
    pub release: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Write Cargo outputs to this directory"
    )]
    pub target_dir: Option<PathBuf>,

    #[arg(
        last = true,
        num_args = 0..,
        allow_hyphen_values = true,
        value_name = "CARGO_ARGS",
        help = "Extra arguments passed through to cargo build after --"
    )]
    pub cargo_args: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RpmArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to package for (auto-detected when omitted)"
    )]
    pub arch: Option<Arch>,

    #[arg(
        short = 'p',
        long,
        help = "Workspace member to package when the project has multiple packages"
    )]
    pub package: Option<String>,

    #[arg(long, help = "Build package inputs in release mode before packaging")]
    pub cargo_release: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Packaging root directory. Defaults to <workspace>/tizen"
    )]
    pub packaging_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "DIR",
        help = "Directory to write generated RPM artifacts into"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        help = "Skip the build step and package the existing binary outputs"
    )]
    pub no_build: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TpkArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to package for (auto-detected when omitted)"
    )]
    pub arch: Option<Arch>,

    #[arg(
        short = 'p',
        long,
        help = "Workspace member to package when the project has multiple packages"
    )]
    pub package: Option<String>,

    #[arg(long, help = "Build package inputs in release mode before packaging")]
    pub cargo_release: bool,

    #[arg(
        long,
        help = "Skip the build step and package the existing binary outputs"
    )]
    pub no_build: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Packaging root directory. Defaults to <workspace>/tizen"
    )]
    pub packaging_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "DIR",
        help = "Directory to write generated TPK artifacts into"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        help = "TPK signing profile from Tizen Studio Certificate Manager"
    )]
    pub sign: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct DevicesArgs {
    #[arg(
        long,
        help = "Include offline, unauthorized, and non-Tizen devices in the output"
    )]
    pub all: bool,
}

#[derive(Debug, Clone, Args)]
pub struct InstallArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to package for before install (auto-detected when omitted)"
    )]
    pub arch: Option<Arch>,

    #[arg(
        short = 'p',
        long,
        help = "Workspace member to package when the project has multiple packages"
    )]
    pub package: Option<String>,

    #[arg(short = 'd', long, help = "Target device ID from cargo tizen devices")]
    pub device: Option<String>,

    #[arg(long, help = "Build package inputs in release mode before packaging")]
    pub cargo_release: bool,

    #[arg(
        long,
        help = "Skip the build step and package the existing binary outputs"
    )]
    pub no_build: bool,

    #[arg(
        long,
        value_name = "PATH",
        help = "Packaging root directory. Defaults to <workspace>/tizen"
    )]
    pub packaging_dir: Option<PathBuf>,

    #[arg(
        long,
        value_name = "DIR",
        help = "Directory to write generated TPK artifacts into"
    )]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        help = "TPK signing profile from Tizen Studio Certificate Manager"
    )]
    pub sign: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Install an existing TPK instead of building and packaging one"
    )]
    pub tpk: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(
        short = 'A',
        long,
        help = "Check one target architecture instead of all supported architectures"
    )]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone, Args)]
pub struct FixArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture to repair. If omitted, applies fixes for all supported architectures"
    )]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[arg(
        long,
        help = "Set the default TPK signing profile stored in user config"
    )]
    pub sign: Option<String>,

    #[arg(long, help = "Print the current persistent configuration values")]
    pub show: bool,
}

#[derive(Debug, Clone, Args)]
pub struct CleanArgs {
    #[arg(long, help = "Remove cached sysroots")]
    pub sysroot: bool,

    #[arg(long, help = "Remove generated build and packaging outputs")]
    pub build: bool,

    #[arg(long, help = "Remove both build outputs and cached sysroots")]
    pub all: bool,

    #[arg(short = 'A', long, help = "Limit cleanup to one target architecture")]
    pub arch: Option<Arch>,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    fn render_help(command: &mut clap::Command) -> String {
        let mut output = Vec::new();
        command.write_long_help(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn root_help_uses_cargo_subcommand_name_and_describes_commands() {
        let mut command = Cli::command();
        let help = render_help(&mut command);

        assert!(help.contains("Usage: cargo tizen [OPTIONS] <COMMAND>"));
        assert!(help.contains("Prepare and cache a Tizen sysroot for cross-compilation"));
        assert!(help.contains("Build or reuse a TPK and install it on a connected device"));
        assert!(help.contains("Quick start:"));
        assert!(help.contains("cargo tizen doctor"));
    }

    #[test]
    fn build_help_includes_examples_and_forwarded_cargo_args() {
        let mut command = Cli::command();
        let mut build = command.find_subcommand_mut("build").unwrap().clone();
        let help = render_help(&mut build);

        assert!(help.contains("Cross-build the current Rust project for a Tizen target"));
        assert!(help.contains("Extra arguments passed through to cargo build after --"));
        assert!(help.contains("cargo tizen build -A aarch64 --release"));
        assert!(help.contains("cargo tizen build -A armv7l -- --features my_feature"));
    }
}
