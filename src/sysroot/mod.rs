use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::arch::Arch;
use crate::cli::SetupArgs;
use crate::context::AppContext;

pub mod cache;
pub mod provider;
pub mod repo;
pub mod rootstrap;
pub mod validate;

use cache::{CacheKey, CacheMeta, STATE_READY};
use provider::{ProviderKind, SetupRequest, provider_for};

#[derive(Debug, Clone)]
pub struct ResolvedSysroot {
    pub entry_dir: PathBuf,
    pub sysroot_dir: PathBuf,
    pub profile: String,
    pub platform_version: String,
    pub provider: ProviderKind,
}

pub fn run_setup(ctx: &AppContext, args: &SetupArgs) -> Result<()> {
    let profile = args.profile.clone().unwrap_or_else(|| ctx.config.profile());
    let platform_version = args
        .platform_version
        .clone()
        .unwrap_or_else(|| ctx.config.platform_version());
    let provider = args
        .provider
        .unwrap_or_else(|| ctx.config.default_provider());

    let request = SetupRequest {
        arch: args.arch,
        profile: profile.clone(),
        platform_version: platform_version.clone(),
        sdk_root_override: args.sdk_root.clone().or_else(|| ctx.config.sdk_root()),
    };

    let provider_impl = provider_for(provider);
    let selected_provider_kind = provider_impl.kind();
    let fingerprint = provider_impl.fingerprint(&request)?;
    let cache_root = ctx.config.cache_root();
    let key = CacheKey::new(&request, provider, &fingerprint);
    let final_entry = cache::entry_path(&cache_root, &key);

    if cache::is_ready(&final_entry)? && !args.force {
        ctx.info(format!(
            "sysroot cache hit: {} ({})",
            final_entry.display(),
            args.arch
        ));
        return Ok(());
    }

    let _lock = cache::acquire_lock(&final_entry)?;
    if cache::is_ready(&final_entry)? && !args.force {
        ctx.info(format!(
            "sysroot cache became ready while waiting: {}",
            final_entry.display()
        ));
        return Ok(());
    }

    if args.force && final_entry.exists() {
        fs::remove_dir_all(&final_entry).with_context(|| {
            format!(
                "failed to remove existing sysroot cache entry: {}",
                final_entry.display()
            )
        })?;
    }

    let temp_entry = cache::temp_entry_path(&final_entry);
    if temp_entry.exists() {
        fs::remove_dir_all(&temp_entry)
            .with_context(|| format!("failed to clean temp cache dir: {}", temp_entry.display()))?;
    }
    fs::create_dir_all(&temp_entry)
        .with_context(|| format!("failed to create temp cache dir: {}", temp_entry.display()))?;

    let sysroot_dir = cache::sysroot_dir(&temp_entry);
    provider_impl
        .prepare(&request, &sysroot_dir)
        .context("failed to prepare sysroot from provider")?;
    validate::validate(&sysroot_dir)?;

    let meta = CacheMeta::new(&request, provider, &fingerprint);
    cache::write_meta(&temp_entry, &meta)?;
    cache::write_state(&temp_entry, STATE_READY)?;

    if final_entry.exists() {
        fs::remove_dir_all(&final_entry).with_context(|| {
            format!(
                "failed to replace existing sysroot cache entry: {}",
                final_entry.display()
            )
        })?;
    }

    fs::rename(&temp_entry, &final_entry).with_context(|| {
        format!(
            "failed to promote sysroot cache entry {} -> {}",
            temp_entry.display(),
            final_entry.display()
        )
    })?;

    ctx.info(format!(
        "sysroot prepared: {} (provider: {})",
        final_entry.display(),
        selected_provider_kind
    ));
    Ok(())
}

pub fn resolve_for_build(ctx: &AppContext, arch: Arch) -> Result<ResolvedSysroot> {
    let profile = ctx.config.profile();
    let platform_version = ctx.config.platform_version();
    let provider = ctx.config.default_provider();

    let request = SetupRequest {
        arch,
        profile: profile.clone(),
        platform_version: platform_version.clone(),
        sdk_root_override: ctx.config.sdk_root(),
    };
    let fingerprint = provider_for(provider).fingerprint(&request)?;
    let cache_root = ctx.config.cache_root();
    let key = CacheKey::new(&request, provider, &fingerprint);
    let entry_dir = cache::entry_path(&cache_root, &key);
    let sysroot_dir = cache::sysroot_dir(&entry_dir);

    if !cache::is_ready(&entry_dir)? {
        bail!(
            "missing sysroot cache for arch {}. run: cargo tizen setup -A {} --profile {} --platform-version {}",
            arch,
            arch,
            profile,
            platform_version
        );
    }

    validate::validate(&sysroot_dir)?;
    Ok(ResolvedSysroot {
        entry_dir,
        sysroot_dir,
        profile,
        platform_version,
        provider,
    })
}
