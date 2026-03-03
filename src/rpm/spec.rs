use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct SpecInput {
    pub package_name: String,
    pub version: String,
    pub release: String,
    pub summary: String,
    pub license: String,
    pub rpm_arch: String,
    pub binary_name: String,
}

pub fn write_spec(path: &Path, input: &SpecInput) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create spec directory {}", parent.display()))?;
    }
    fs::write(path, render(input))
        .with_context(|| format!("failed to write spec file {}", path.display()))
}

fn render(input: &SpecInput) -> String {
    format!(
        "Name:           {name}\n\
Version:        {version}\n\
Release:        {release}%{{?dist}}\n\
Summary:        {summary}\n\
License:        {license}\n\
BuildArch:      {arch}\n\
Source0:        {binary_name}\n\
\n\
%description\n\
{summary}\n\
\n\
%prep\n\
\n\
%build\n\
\n\
%install\n\
install -Dm0755 %{{SOURCE0}} %{{buildroot}}/usr/bin/{binary_name}\n\
\n\
%files\n\
/usr/bin/{binary_name}\n",
        name = input.package_name,
        version = input.version,
        release = input.release,
        summary = input.summary,
        license = input.license,
        arch = input.rpm_arch,
        binary_name = input.binary_name
    )
}

#[cfg(test)]
mod tests {
    use super::{SpecInput, render};

    #[test]
    fn generated_spec_omits_changelog() {
        let input = SpecInput {
            package_name: "demo".to_string(),
            version: "1.2.3".to_string(),
            release: "1".to_string(),
            summary: "demo summary".to_string(),
            license: "Apache-2.0".to_string(),
            rpm_arch: "armv7l".to_string(),
            binary_name: "demo".to_string(),
        };

        let spec = render(&input);
        assert!(spec.contains("BuildArch:      armv7l"));
        assert!(spec.contains("/usr/bin/demo"));
        assert!(!spec.contains("%changelog"));
    }
}
