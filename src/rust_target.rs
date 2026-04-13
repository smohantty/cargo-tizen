use std::path::Path;

use anyhow::Result;

use crate::arch::Arch;
use crate::context::AppContext;
use crate::sysroot;
use crate::sysroot::provider::{ProviderKind, SetupRequest};
use crate::sysroot::rootstrap;

const ARM_SOFT_FLOAT_TARGET: &str = "armv7-unknown-linux-gnueabi";
const ARM_HARD_FLOAT_TARGET: &str = "armv7-unknown-linux-gnueabihf";

pub fn resolve_for_arch(ctx: &AppContext, arch: Arch) -> Result<String> {
    resolve_with_sysroot_hint(ctx, arch, None)
}

pub fn resolve_with_sysroot_hint(
    ctx: &AppContext,
    arch: Arch,
    sysroot_hint: Option<&Path>,
) -> Result<String> {
    if let Some(explicit) = ctx.config.rust_target_override_for(arch) {
        return Ok(explicit);
    }

    if arch != Arch::Armv7l || ctx.config.default_provider() != ProviderKind::Rootstrap {
        return Ok(ctx.config.rust_target_for(arch));
    }

    if let Some(root) = sysroot_hint {
        if let Some(detected) = infer_armv7_target_from_sysroot_root(root) {
            return Ok(detected.to_string());
        }
    }

    let (profile, platform_version) = sysroot::resolve_profile_platform_for_arch(ctx, arch)?;
    let req = SetupRequest {
        arch,
        profile,
        platform_version,
        sdk_root_override: ctx.config.sdk_root(),
    };
    if let Ok(resolved) = rootstrap::resolve_rootstrap(&req) {
        if let Some(detected) = infer_armv7_target_from_sysroot_root(&resolved.root_path) {
            return Ok(detected.to_string());
        }
    }

    Ok(ctx.config.rust_target_for(arch))
}

fn infer_armv7_target_from_sysroot_root(root: &Path) -> Option<&'static str> {
    let gnu_headers = root.join("usr").join("include").join("gnu");
    let has_soft = gnu_headers.join("stubs-soft.h").is_file();
    let has_hard = gnu_headers.join("stubs-hard.h").is_file();

    match (has_soft, has_hard) {
        (true, false) => Some(ARM_SOFT_FLOAT_TARGET),
        (false, true) => Some(ARM_HARD_FLOAT_TARGET),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ARM_HARD_FLOAT_TARGET, ARM_SOFT_FLOAT_TARGET, infer_armv7_target_from_sysroot_root,
    };

    #[test]
    fn infers_soft_float_target() {
        let temp = temp_dir();
        let gnu = temp.join("usr").join("include").join("gnu");
        fs::create_dir_all(&gnu).expect("should create gnu include dir");
        fs::write(gnu.join("stubs-soft.h"), "").expect("should create stubs-soft.h");

        assert_eq!(
            infer_armv7_target_from_sysroot_root(&temp),
            Some(ARM_SOFT_FLOAT_TARGET)
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn infers_hard_float_target() {
        let temp = temp_dir();
        let gnu = temp.join("usr").join("include").join("gnu");
        fs::create_dir_all(&gnu).expect("should create gnu include dir");
        fs::write(gnu.join("stubs-hard.h"), "").expect("should create stubs-hard.h");

        assert_eq!(
            infer_armv7_target_from_sysroot_root(&temp),
            Some(ARM_HARD_FLOAT_TARGET)
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn returns_none_when_abi_is_ambiguous() {
        let temp = temp_dir();
        let gnu = temp.join("usr").join("include").join("gnu");
        fs::create_dir_all(&gnu).expect("should create gnu include dir");
        fs::write(gnu.join("stubs-soft.h"), "").expect("should create stubs-soft.h");
        fs::write(gnu.join("stubs-hard.h"), "").expect("should create stubs-hard.h");

        assert_eq!(infer_armv7_target_from_sysroot_root(&temp), None);

        let _ = fs::remove_dir_all(temp);
    }

    fn temp_dir() -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        Path::new("/tmp").join(format!("cargo-tizen-rust-target-test-{}", ts))
    }

    #[test]
    fn returns_none_when_gnu_dir_missing() {
        let temp = temp_dir();
        std::fs::create_dir_all(temp.join("usr").join("include")).expect("create include dir");
        // no gnu/ subdirectory at all
        assert_eq!(infer_armv7_target_from_sysroot_root(&temp), None);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn returns_none_when_gnu_dir_is_empty() {
        let temp = temp_dir();
        let gnu = temp.join("usr").join("include").join("gnu");
        std::fs::create_dir_all(&gnu).expect("create gnu dir");
        // gnu/ exists but has neither stubs file
        assert_eq!(infer_armv7_target_from_sysroot_root(&temp), None);
        let _ = std::fs::remove_dir_all(temp);
    }
}
