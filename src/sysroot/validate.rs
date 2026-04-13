use std::path::Path;

use anyhow::{Result, bail};

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
    contains_file_name_impl(root, &predicate)
}

fn contains_file_name_impl<F>(root: &Path, predicate: &F) -> bool
where
    F: Fn(&str) -> bool + ?Sized,
{
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_file() {
            if let Some(name) = path.file_name().and_then(|v| v.to_str()) {
                if predicate(name) {
                    return true;
                }
            }
            continue;
        }

        if file_type.is_dir() && contains_file_name_impl(&path, predicate) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::validate;

    fn setup_valid_sysroot(root: &std::path::Path) {
        fs::create_dir_all(root.join("usr/include")).unwrap();
        fs::create_dir_all(root.join("usr/lib/pkgconfig")).unwrap();
        // crt1.o, crti.o, libc.so
        fs::write(root.join("usr/lib/crt1.o"), b"").unwrap();
        fs::write(root.join("usr/lib/crti.o"), b"").unwrap();
        fs::write(root.join("usr/lib/libc.so"), b"").unwrap();
    }

    #[test]
    fn valid_sysroot_passes() {
        let dir = std::env::temp_dir().join(format!("ct-validate-ok-{}", std::process::id()));
        setup_valid_sysroot(&dir);
        validate(&dir).expect("valid sysroot should pass");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_nonexistent_path() {
        let err = validate(std::path::Path::new("/nonexistent/sysroot/xyz"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a directory"));
    }

    #[test]
    fn rejects_missing_include_dir() {
        let dir = std::env::temp_dir().join(format!("ct-validate-inc-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/lib/pkgconfig")).unwrap();
        fs::write(dir.join("usr/lib/crt1.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/crti.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/libc.so"), b"").unwrap();
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("missing include directory"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_lib_dir() {
        let dir = std::env::temp_dir().join(format!("ct-validate-lib-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        // no lib dirs at all
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("missing library directories"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_pkgconfig() {
        let dir = std::env::temp_dir().join(format!("ct-validate-pkg-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        fs::create_dir_all(dir.join("usr/lib")).unwrap();
        // no pkgconfig
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("missing pkg-config"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_crt1() {
        let dir = std::env::temp_dir().join(format!("ct-validate-crt1-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        fs::create_dir_all(dir.join("usr/lib/pkgconfig")).unwrap();
        fs::write(dir.join("usr/lib/crti.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/libc.so"), b"").unwrap();
        // no crt1.o or Scrt1.o
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("crt1.o"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_crti() {
        let dir = std::env::temp_dir().join(format!("ct-validate-crti-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        fs::create_dir_all(dir.join("usr/lib/pkgconfig")).unwrap();
        fs::write(dir.join("usr/lib/crt1.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/libc.so"), b"").unwrap();
        // no crti.o
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("crti.o"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_libc() {
        let dir = std::env::temp_dir().join(format!("ct-validate-libc-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        fs::create_dir_all(dir.join("usr/lib/pkgconfig")).unwrap();
        fs::write(dir.join("usr/lib/crt1.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/crti.o"), b"").unwrap();
        // no libc.so
        let err = validate(&dir).unwrap_err().to_string();
        assert!(err.contains("libc.so"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scrt1_is_accepted_as_alternative() {
        let dir = std::env::temp_dir().join(format!("ct-validate-scrt-{}", std::process::id()));
        fs::create_dir_all(dir.join("usr/include")).unwrap();
        fs::create_dir_all(dir.join("usr/lib/pkgconfig")).unwrap();
        fs::write(dir.join("usr/lib/Scrt1.o"), b"").unwrap(); // Scrt1.o instead of crt1.o
        fs::write(dir.join("usr/lib/crti.o"), b"").unwrap();
        fs::write(dir.join("usr/lib/libc.so"), b"").unwrap();
        validate(&dir).expect("Scrt1.o should be accepted");
        let _ = fs::remove_dir_all(&dir);
    }
}
