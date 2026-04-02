use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::arch::Arch;
use crate::context::AppContext;
use crate::device::{self, TizenDevice};
use crate::output::{color_enabled, colorize};

pub fn resolve_arch(ctx: &AppContext, explicit: Option<Arch>, command_name: &str) -> Result<Arch> {
    if let Some(arch) = explicit {
        return Ok(arch);
    }

    if let Some(arch) = configured_default_arch(ctx)? {
        if should_announce_selection(command_name) {
            ctx.info(format!(
                "{} {} (from [default].arch)",
                arch_status("Arch"),
                arch
            ));
        }
        return Ok(arch);
    }

    if let Some(arch) = single_configured_arch(ctx) {
        if should_announce_selection(command_name) {
            ctx.info(format!(
                "{} {} (from [arch.*] config)",
                arch_status("Arch"),
                arch
            ));
        }
        return Ok(arch);
    }

    match detect_arch_from_connected_devices(ctx) {
        DeviceArchSelection::Single(arch) => {
            if should_announce_selection(command_name) {
                ctx.info(format!(
                    "{} {} (from connected device)",
                    arch_status("Arch"),
                    arch
                ));
            }
            return Ok(arch);
        }
        DeviceArchSelection::Ambiguous(arches) => {
            let values = arches
                .iter()
                .map(|arch| arch.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let examples = arches
                .iter()
                .map(|arch| {
                    format!(
                        "  cargo tizen {command_name} -A {:<10} ({})",
                        arch.as_str(),
                        arch.rust_target()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "multiple device architectures detected ({values})\n\n\
                 pick one:\n{examples}"
            );
        }
        DeviceArchSelection::None => {}
    }

    let project_config_exists = ctx.workspace_root.join(".cargo-tizen.toml").is_file();
    let arch_lines = Arch::all()
        .iter()
        .map(|arch| {
            format!(
                "  cargo tizen {command_name} -A {:<10} ({})",
                arch.as_str(),
                arch.rust_target()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !project_config_exists {
        bail!(
            "project not initialized for cargo-tizen\n\n\
             get started:\n\
             \x20 cargo tizen init              create project config\n\
             \x20 cargo tizen init --rpm        also scaffold RPM packaging\n\
             \x20 cargo tizen init --tpk        also scaffold TPK packaging\n\n\
             then run:\n{arch_lines}"
        )
    }

    bail!(
        "target architecture required for `cargo tizen {command_name}`\n\n\
         pick one:\n{arch_lines}\n\n\
         or set a default in .cargo-tizen.toml:\n\
         [default]\n\
         arch = \"aarch64\""
    )
}

fn should_announce_selection(_command_name: &str) -> bool {
    false
}

fn arch_status(label: &str) -> String {
    colorize(color_enabled(), "1;92", &format!("{label:>15}"))
}

fn configured_default_arch(ctx: &AppContext) -> Result<Option<Arch>> {
    parse_arch_value(ctx.config.default.arch.as_deref(), "[default].arch")
}

fn single_configured_arch(ctx: &AppContext) -> Option<Arch> {
    let mut arches = BTreeSet::new();
    for key in ctx.config.arch.keys() {
        if let Some(arch) = Arch::parse(key) {
            arches.insert(arch);
        }
    }

    if arches.len() == 1 {
        return arches.into_iter().next();
    }

    None
}

fn detect_arch_from_connected_devices(ctx: &AppContext) -> DeviceArchSelection {
    let devices = match device::discover_devices(ctx) {
        Ok(devices) => devices,
        Err(err) => {
            ctx.debug(format!("arch auto-detection via devices skipped: {}", err));
            return DeviceArchSelection::None;
        }
    };

    detect_arch_from_devices(&devices)
}

fn detect_arch_from_devices(devices: &[TizenDevice]) -> DeviceArchSelection {
    let mut arches = BTreeSet::new();
    for device in devices
        .iter()
        .filter(|device| device.state == "device" && device.is_tizen)
    {
        if let Some(cpu_arch) = device.cpu_arch.as_deref().and_then(Arch::parse) {
            arches.insert(cpu_arch);
        }
    }

    match arches.len() {
        0 => DeviceArchSelection::None,
        1 => DeviceArchSelection::Single(*arches.first().expect("single arch set is non-empty")),
        _ => DeviceArchSelection::Ambiguous(arches.into_iter().collect()),
    }
}

fn parse_arch_value(value: Option<&str>, source: &str) -> Result<Option<Arch>> {
    let Some(raw) = value else {
        return Ok(None);
    };

    if let Some(arch) = Arch::parse(raw) {
        return Ok(Some(arch));
    }

    bail!(
        "invalid {} value `{}`. expected one of: {}",
        source,
        raw,
        supported_arch_list()
    )
}

fn supported_arch_list() -> String {
    Arch::all()
        .iter()
        .map(|arch| arch.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeviceArchSelection {
    None,
    Single(Arch),
    Ambiguous(Vec<Arch>),
}

#[cfg(test)]
mod tests {
    use crate::device::TizenDevice;

    use super::{DeviceArchSelection, detect_arch_from_devices, parse_arch_value};
    use crate::arch::Arch;

    #[test]
    fn parses_configured_default_arch() {
        assert_eq!(
            parse_arch_value(Some("arm64"), "[default].arch")
                .expect("parsing arm64 should succeed"),
            Some(Arch::Aarch64)
        );
    }

    #[test]
    fn rejects_invalid_default_arch() {
        let err = parse_arch_value(Some("mips"), "[default].arch")
            .expect_err("invalid arch should be rejected")
            .to_string();
        assert!(err.contains("invalid [default].arch value"));
    }

    #[test]
    fn detects_single_device_arch() {
        let devices = vec![device_with_arch("arm")];
        assert_eq!(
            detect_arch_from_devices(&devices),
            DeviceArchSelection::Single(Arch::Armv7l)
        );
    }

    #[test]
    fn detects_ambiguous_device_arch() {
        let devices = vec![device_with_arch("arm"), device_with_arch("aarch64")];
        assert_eq!(
            detect_arch_from_devices(&devices),
            DeviceArchSelection::Ambiguous(vec![Arch::Armv7l, Arch::Aarch64])
        );
    }

    fn device_with_arch(cpu_arch: &str) -> TizenDevice {
        TizenDevice {
            id: "test".to_string(),
            state: "device".to_string(),
            model: "model".to_string(),
            profile: Some("mobile".to_string()),
            cpu_arch: Some(cpu_arch.to_string()),
            secure_protocol: false,
            is_tizen: true,
        }
    }
}
