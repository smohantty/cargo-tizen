use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::arch::Arch;
use crate::sysroot::provider::ProviderKind;

#[derive(Debug, Parser)]
#[command(
    name = "cargo-tizen",
    about = "Build Rust projects for Tizen and generate RPM/TPK packages",
    version
)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Setup(SetupArgs),
    Build(BuildArgs),
    Rpm(RpmArgs),
    Tpk(TpkArgs),
    Devices(DevicesArgs),
    Run(RunArgs),
    Doctor(DoctorArgs),
    Fix(FixArgs),
    Clean(CleanArgs),
}

#[derive(Debug, Clone, Args)]
pub struct SetupArgs {
    #[arg(short = 'A', long, help = "Target architecture (auto-detected when omitted)")]
    pub arch: Option<Arch>,

    #[arg(long)]
    pub profile: Option<String>,

    #[arg(long)]
    pub platform_version: Option<String>,

    #[arg(long, value_enum)]
    pub provider: Option<ProviderKind>,

    #[arg(long)]
    pub sdk_root: Option<PathBuf>,

    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, Args)]
pub struct BuildArgs {
    #[arg(short = 'A', long, help = "Target architecture (auto-detected when omitted)")]
    pub arch: Option<Arch>,

    #[arg(long)]
    pub release: bool,

    #[arg(long)]
    pub target_dir: Option<PathBuf>,

    #[arg(last = true, num_args = 0.., allow_hyphen_values = true)]
    pub cargo_args: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RpmArgs {
    #[arg(short = 'A', long, help = "Target architecture (auto-detected when omitted)")]
    pub arch: Option<Arch>,

    #[arg(long, default_value = "1")]
    pub release: String,

    #[arg(long)]
    pub cargo_release: bool,

    #[arg(long)]
    pub spec: Option<PathBuf>,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub no_build: bool,
}

#[derive(Debug, Clone, Args)]
pub struct TpkArgs {
    #[arg(short = 'A', long, help = "Target architecture (auto-detected when omitted)")]
    pub arch: Option<Arch>,

    #[arg(long)]
    pub cargo_release: bool,

    #[arg(long)]
    pub no_build: bool,

    #[arg(long)]
    pub manifest: Option<PathBuf>,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub sign: Option<String>,

    #[arg(long)]
    pub reference: Option<PathBuf>,

    #[arg(long)]
    pub extra_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct DevicesArgs {
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    #[arg(short = 'A', long, help = "Target architecture (auto-detected when omitted)")]
    pub arch: Option<Arch>,

    #[arg(short = 'd', long)]
    pub device: Option<String>,

    #[arg(long)]
    pub cargo_release: bool,

    #[arg(long)]
    pub no_build: bool,

    #[arg(long)]
    pub manifest: Option<PathBuf>,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub sign: Option<String>,

    #[arg(long)]
    pub reference: Option<PathBuf>,

    #[arg(long)]
    pub extra_dir: Option<PathBuf>,

    #[arg(long)]
    pub tpk: Option<PathBuf>,

    #[arg(long)]
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(short = 'A', long)]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone, Args)]
pub struct FixArgs {
    #[arg(
        short = 'A',
        long,
        help = "Target architecture. If omitted, applies fixes for all supported architectures"
    )]
    pub arch: Option<Arch>,
}

#[derive(Debug, Clone, Args)]
pub struct CleanArgs {
    #[arg(long)]
    pub sysroot: bool,

    #[arg(long)]
    pub build: bool,

    #[arg(long)]
    pub all: bool,

    #[arg(short = 'A', long)]
    pub arch: Option<Arch>,
}
