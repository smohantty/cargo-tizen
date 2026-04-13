use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::ConfigArgs;
use crate::config::{self, Config};
use crate::context::AppContext;
use crate::output::{color_enabled, colorize};

pub fn run_config(ctx: &AppContext, args: &ConfigArgs) -> Result<()> {
    if let Some(sign) = &args.sign {
        set_sign(ctx, sign)?;
        if args.show {
            show_config(ctx);
        }
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
        basic_toml::from_str::<Config>(&raw)
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

    let serialized = basic_toml::to_string(&cfg).context("failed to serialize config")?;
    fs::write(&path, serialized).with_context(|| format!("failed to write {}", path.display()))?;

    ctx.info(format!("wrote {}", path.display()));
    Ok(())
}

fn show_config(ctx: &AppContext) {
    let use_color = color_enabled();
    let cfg = &ctx.config;

    // Config file locations
    let user_path = config::user_config_path();
    let project_path = PathBuf::from(".cargo-tizen.toml");

    let label = |s: &str| colorize(use_color, "1", s);
    let dim = |s: &str| colorize(use_color, "2", s);

    if let Some(ref p) = user_path {
        if p.exists() {
            ctx.info(format!("{} {}", dim("user config:"), p.display()));
        }
    }
    if project_path.exists() {
        ctx.info(format!(
            "{} {}",
            dim("project config:"),
            project_path
                .canonicalize()
                .unwrap_or(project_path)
                .display()
        ));
    }

    // [default]
    ctx.info(format!("\n{}", label("[default]")));
    show_field(ctx, "arch", cfg.default.arch.as_deref());
    show_field(ctx, "profile", cfg.default.profile.as_deref());
    show_field(
        ctx,
        "platform_version",
        cfg.default.platform_version.as_deref(),
    );
    show_field(ctx, "provider", cfg.default.provider.as_deref());
    show_field(ctx, "packaging_dir", cfg.default.packaging_dir.as_deref());

    // [sdk]
    ctx.info(format!("\n{}", label("[sdk]")));
    show_field(ctx, "root", cfg.sdk.root.as_deref());

    // [cache]
    ctx.info(format!("\n{}", label("[cache]")));
    ctx.info(format!("  {} = {}", "root", cfg.cache_root().display()));

    // [package]
    ctx.info(format!("\n{}", label("[package]")));
    show_field(ctx, "name", cfg.package.name());
    if let Some(pkgs) = cfg.package_names() {
        ctx.info(format!("  packages = {:?}", pkgs));
    } else {
        show_field::<&str>(ctx, "packages", None);
    }

    // [tpk]
    ctx.info(format!("\n{}", label("[tpk]")));
    show_field(ctx, "sign", cfg.tpk.sign.as_deref());

    // [release]
    ctx.info(format!("\n{}", label("[release]")));
    match &cfg.release.arches {
        Some(arches) => ctx.info(format!("  arches = {:?}", arches)),
        None => show_field::<&str>(ctx, "arches", None),
    }
    show_field(ctx, "tag_format", cfg.release.tag_format.as_deref());

    // [arch.*] overrides
    if !cfg.arch.is_empty() {
        for (name, arch_cfg) in &cfg.arch {
            ctx.info(format!("\n{}", label(&format!("[arch.{name}]"))));
            show_field(ctx, "rust_target", arch_cfg.rust_target.as_deref());
            show_field(ctx, "linker", arch_cfg.linker.as_deref());
            show_field(ctx, "cc", arch_cfg.cc.as_deref());
            show_field(ctx, "cxx", arch_cfg.cxx.as_deref());
            show_field(ctx, "ar", arch_cfg.ar.as_deref());
            show_field(ctx, "tizen_cli_arch", arch_cfg.tizen_cli_arch.as_deref());
            show_field(
                ctx,
                "tizen_build_arch",
                arch_cfg.tizen_build_arch.as_deref(),
            );
            show_field(ctx, "rpm_build_arch", arch_cfg.rpm_build_arch.as_deref());
        }
    }
}

fn show_field<V: std::fmt::Display>(ctx: &AppContext, key: &str, value: Option<V>) {
    match value {
        Some(v) => ctx.info(format!("  {key} = {v}")),
        None => {
            let use_color = color_enabled();
            ctx.info(format!(
                "  {key} = {}",
                colorize(use_color, "2", "(not set)")
            ));
        }
    }
}
