mod arch;
mod arch_detect;
mod cargo_runner;
mod clean;
mod cli;
mod config;
mod config_cmd;
mod context;
mod device;
mod doctor;
mod fix;
mod init_cmd;
mod install_cmd;
mod output;
mod package_select;
mod packaging;
mod rpm;
mod rust_target;
mod sdk;
mod sysroot;
mod tool_env;
mod tpk;

use std::ffi::OsString;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::context::AppContext;

fn parse_cli() -> Cli {
    let mut args: Vec<OsString> = std::env::args_os().collect();
    if args.get(1).and_then(|arg| arg.to_str()) == Some("tizen") {
        args.remove(1);
    }
    Cli::parse_from(args)
}

fn main() -> Result<()> {
    let cli = parse_cli();
    let config = config::Config::load()?;
    let ctx = AppContext::new(config);

    match cli.command {
        Command::Setup(args) => sysroot::run_setup(&ctx, &args)?,
        Command::Init(args) => init_cmd::run_init(&ctx, &args)?,
        Command::Build(args) => cargo_runner::run_build(&ctx, &args)?,
        Command::Rpm(args) => rpm::run_rpm(&ctx, &args)?,
        Command::Tpk(args) => tpk::run_tpk(&ctx, &args)?,
        Command::Devices(args) => device::run_devices(&ctx, &args)?,
        Command::Install(args) => install_cmd::run_install(&ctx, &args)?,
        Command::Doctor(args) => doctor::run_doctor(&ctx, &args)?,
        Command::Fix(args) => fix::run_fix(&ctx, &args)?,
        Command::Clean(args) => clean::run_clean(&ctx, &args)?,
        Command::Config(args) => config_cmd::run_config(&ctx, &args)?,
    }

    Ok(())
}
