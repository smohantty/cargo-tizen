use std::path::Path;

use anyhow::{Result, bail};
use walkdir::WalkDir;

pub fn validate(sysroot_dir: &Path) -> Result<()> {
    if !sysroot_dir.is_dir() {
        bail!("sysroot path is not a directory: {}", sysroot_dir.display());
    }

    let include_dir = sysroot_dir.join("usr/include");
    if !include_dir.is_dir() {
        bail!(
            "sysroot is missing include directory: {}",
            include_dir.display()
        );
    }

    let has_lib_dir = ["usr/lib", "usr/lib64", "lib", "lib64"]
        .iter()
        .any(|rel| sysroot_dir.join(rel).is_dir());
    if !has_lib_dir {
        bail!(
            "sysroot is missing library directories under {}",
            sysroot_dir.display()
        );
    }

    let has_pkgconfig = [
        "usr/lib/pkgconfig",
        "usr/lib64/pkgconfig",
        "usr/share/pkgconfig",
    ]
    .iter()
    .any(|rel| sysroot_dir.join(rel).is_dir());
    if !has_pkgconfig {
        bail!(
            "sysroot is missing pkg-config directories under {}",
            sysroot_dir.display()
        );
    }

    let has_crt1 = contains_file_name(sysroot_dir, |name| name == "crt1.o" || name == "Scrt1.o");
    let has_crti = contains_file_name(sysroot_dir, |name| name == "crti.o");
    let has_libc = contains_file_name(sysroot_dir, |name| {
        name == "libc.so" || name.starts_with("libc.so.")
    });

    if !has_crt1 {
        bail!(
            "sysroot validation failed: crt startup object (crt1.o or Scrt1.o) not found in {}",
            sysroot_dir.display()
        );
    }
    if !has_crti {
        bail!(
            "sysroot validation failed: crti.o not found in {}",
            sysroot_dir.display()
        );
    }
    if !has_libc {
        bail!(
            "sysroot validation failed: libc.so not found in {}",
            sysroot_dir.display()
        );
    }

    Ok(())
}

fn contains_file_name<F>(root: &Path, predicate: F) -> bool
where
    F: Fn(&str) -> bool,
{
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(name) = entry.path().file_name().and_then(|v| v.to_str()) {
            if predicate(name) {
                return true;
            }
        }
    }
    false
}
