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
    sibling_with_suffix(entry_path, &format!("tmp-{pid}"))
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

#[derive(Debug)]
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

    let lock_path = sibling_with_suffix(entry_path, "lock");

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

fn sibling_with_suffix(entry_path: &Path, suffix: &str) -> PathBuf {
    let parent = entry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = entry_path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("entry");
    parent.join(format!("{name}.{suffix}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        CacheKey, STATE_READY, acquire_lock, entry_path, is_ready, sanitize_component, sysroot_dir,
        write_state,
    };
    use super::{sibling_with_suffix, temp_entry_path};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn suffix_generation_preserves_dotted_name() {
        let entry = Path::new("/tmp/rootstrap-mobile-9.0-armv7l-v1");
        let lock = sibling_with_suffix(entry, "lock");
        assert_eq!(
            lock.to_string_lossy(),
            "/tmp/rootstrap-mobile-9.0-armv7l-v1.lock"
        );
    }

    #[test]
    fn temp_path_generation_preserves_dotted_name() {
        let entry = Path::new("/tmp/rootstrap-mobile-9.0-armv7l-v1");
        let temp = temp_entry_path(entry);
        assert!(
            temp.to_string_lossy()
                .starts_with("/tmp/rootstrap-mobile-9.0-armv7l-v1.tmp-")
        );
    }

    #[test]
    fn sanitize_component_passes_safe_chars() {
        assert_eq!(sanitize_component("mobile-9.0_v1"), "mobile-9.0_v1");
    }

    #[test]
    fn sanitize_component_replaces_special_chars() {
        assert_eq!(sanitize_component("a/b:c d"), "a_b_c_d");
    }

    #[test]
    fn entry_path_builds_correct_hierarchy() {
        let key = CacheKey {
            profile: "mobile".into(),
            platform_version: "10.0".into(),
            arch: "aarch64".into(),
            provider: "rootstrap".into(),
            fingerprint: "fp-v1".into(),
        };
        let path = entry_path(Path::new("/cache"), &key);
        assert_eq!(
            path,
            PathBuf::from("/cache/mobile/10.0/aarch64/rootstrap/fp-v1")
        );
    }

    #[test]
    fn sysroot_dir_appends_sysroot() {
        assert_eq!(
            sysroot_dir(Path::new("/cache/entry")),
            PathBuf::from("/cache/entry/sysroot")
        );
    }

    #[test]
    fn is_ready_returns_false_for_nonexistent() {
        assert!(!is_ready(Path::new("/nonexistent/path/xyz")).unwrap());
    }

    #[test]
    fn write_state_and_is_ready_round_trip() {
        let dir = std::env::temp_dir().join(format!("ct-cache-test-{}", std::process::id()));
        let entry = dir.join("test-entry");
        let sysroot = entry.join("sysroot");
        fs::create_dir_all(&sysroot).unwrap();
        write_state(&entry, STATE_READY).unwrap();
        assert!(is_ready(&entry).unwrap());
        // wrong state
        write_state(&entry, "building").unwrap();
        assert!(!is_ready(&entry).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn acquire_lock_and_drop_cleans_up() {
        let dir = std::env::temp_dir().join(format!("ct-lock-test-{}", std::process::id()));
        let entry = dir.join("test-entry");
        fs::create_dir_all(&dir).unwrap();
        let lock_path = Path::new(&dir).join("test-entry.lock");
        {
            let _lock = acquire_lock(&entry).unwrap();
            assert!(lock_path.exists());
        }
        // After drop, lock file should be removed
        assert!(!lock_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn acquire_lock_fails_when_already_held() {
        let dir = std::env::temp_dir().join(format!("ct-lock-dup-{}", std::process::id()));
        let entry = dir.join("test-entry");
        fs::create_dir_all(&dir).unwrap();
        let _lock = acquire_lock(&entry).unwrap();
        let err = acquire_lock(&entry).unwrap_err();
        assert!(err.to_string().contains("already in progress"));
        let _ = fs::remove_dir_all(&dir);
    }
}
