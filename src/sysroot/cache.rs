use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::sysroot::provider::{ProviderKind, SetupRequest};

pub const STATE_READY: &str = "ready";

#[derive(Debug, Clone)]
pub struct CacheKey {
    pub profile: String,
    pub platform_version: String,
    pub arch: String,
    pub provider: String,
    pub fingerprint: String,
}

impl CacheKey {
    pub fn new(req: &SetupRequest, provider: ProviderKind, fingerprint: &str) -> Self {
        Self {
            profile: req.profile.clone(),
            platform_version: req.platform_version.clone(),
            arch: req.arch.as_str().to_string(),
            provider: provider.to_string(),
            fingerprint: sanitize_component(fingerprint),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheMeta {
    created_unix: u64,
    arch: String,
    profile: String,
    platform_version: String,
    provider: String,
    fingerprint: String,
}

impl CacheMeta {
    pub fn new(req: &SetupRequest, provider: ProviderKind, fingerprint: &str) -> Self {
        let created_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|v| v.as_secs())
            .unwrap_or_default();
        Self {
            created_unix,
            arch: req.arch.as_str().to_string(),
            profile: req.profile.clone(),
            platform_version: req.platform_version.clone(),
            provider: provider.to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }
}

pub fn entry_path(cache_root: &Path, key: &CacheKey) -> PathBuf {
    cache_root
        .join(&key.profile)
        .join(&key.platform_version)
        .join(&key.arch)
        .join(&key.provider)
        .join(&key.fingerprint)
}

pub fn temp_entry_path(entry_path: &Path) -> PathBuf {
    let pid = std::process::id();
    let mut path = entry_path.to_path_buf();
    path.set_extension(format!("tmp-{pid}"));
    path
}

pub fn sysroot_dir(entry_path: &Path) -> PathBuf {
    entry_path.join("sysroot")
}

pub fn is_ready(entry_path: &Path) -> Result<bool> {
    if !entry_path.exists() {
        return Ok(false);
    }

    let state = read_state(entry_path)?;
    Ok(state.as_deref() == Some(STATE_READY) && sysroot_dir(entry_path).is_dir())
}

pub fn write_state(entry_path: &Path, state: &str) -> Result<()> {
    fs::create_dir_all(entry_path).with_context(|| {
        format!(
            "failed to create cache entry directory: {}",
            entry_path.display()
        )
    })?;
    fs::write(state_path(entry_path), state)
        .with_context(|| format!("failed to write state file for {}", entry_path.display()))
}

pub fn write_meta(entry_path: &Path, meta: &CacheMeta) -> Result<()> {
    fs::create_dir_all(entry_path).with_context(|| {
        format!(
            "failed to create cache entry directory: {}",
            entry_path.display()
        )
    })?;
    let meta_path = meta_path(entry_path);
    let raw = serde_json::to_string_pretty(meta).context("failed to serialize cache metadata")?;
    fs::write(&meta_path, raw)
        .with_context(|| format!("failed to write cache metadata: {}", meta_path.display()))
}

pub struct CacheLock {
    lock_path: PathBuf,
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

pub fn acquire_lock(entry_path: &Path) -> Result<CacheLock> {
    let parent = entry_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cache entry has no parent: {}", entry_path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create cache parent dir: {}", parent.display()))?;

    let mut lock_path = entry_path.to_path_buf();
    lock_path.set_extension("lock");

    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock_path);
    match file {
        Ok(_) => Ok(CacheLock { lock_path }),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            bail!(
                "another setup is already in progress for {}",
                entry_path.display()
            )
        }
        Err(err) => {
            Err(err).with_context(|| format!("failed to acquire lock {}", lock_path.display()))
        }
    }
}

fn read_state(entry_path: &Path) -> Result<Option<String>> {
    let path = state_path(entry_path);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read state file: {}", path.display()))?;
    Ok(Some(raw.trim().to_string()))
}

fn state_path(entry_path: &Path) -> PathBuf {
    entry_path.join("state")
}

fn meta_path(entry_path: &Path) -> PathBuf {
    entry_path.join("meta.json")
}

fn sanitize_component(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
