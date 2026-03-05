use std::fs;

use anyhow::{Context, Result};

use crate::cli::ConfigArgs;
use crate::config::{self, Config};
use crate::context::AppContext;

pub fn run_config(ctx: &AppContext, args: &ConfigArgs) -> Result<()> {
    if let Some(sign) = &args.sign {
        set_sign(ctx, sign)?;
    } else {
        show_config(ctx);
    }
    Ok(())
}

fn set_sign(ctx: &AppContext, sign: &str) -> Result<()> {
    let path = config::user_config_path()
        .ok_or_else(|| anyhow::anyhow!("unable to determine user config directory"))?;

    let mut cfg = if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str::<Config>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        Config::default()
    };

    if sign.is_empty() {
        cfg.tpk.sign = None;
        ctx.info("cleared tpk.sign");
    } else {
        cfg.tpk.sign = Some(sign.to_string());
        ctx.info(format!("set tpk.sign = {sign}"));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let serialized = toml::to_string_pretty(&cfg).context("failed to serialize config")?;
    fs::write(&path, serialized).with_context(|| format!("failed to write {}", path.display()))?;

    ctx.info(format!("wrote {}", path.display()));
    Ok(())
}

fn show_config(ctx: &AppContext) {
    match ctx.config.tpk_sign() {
        Some(sign) => ctx.info(format!("tpk.sign = {sign}")),
        None => ctx.info("tpk.sign = (not set)"),
    }
}
