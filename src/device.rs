use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::cli::DevicesArgs;
use crate::context::AppContext;
use crate::output::{cargo_status, color_enabled, colorize};
use crate::sdk::TizenSdk;
use crate::tool_env;

#[derive(Debug, Clone)]
pub struct TizenDevice {
    pub id: String,
    pub state: String,
    pub model: String,
    pub profile: Option<String>,
    pub cpu_arch: Option<String>,
    pub secure_protocol: bool,
    pub is_tizen: bool,
}

#[derive(Debug, Clone)]
struct SdbEntry {
    id: String,
    state: String,
    model: String,
}

pub fn run_devices(ctx: &AppContext, args: &DevicesArgs) -> Result<()> {
    let devices = discover_devices(ctx)?;
    let use_color = color_enabled();

    if devices.is_empty() {
        ctx.info("no devices detected via sdb");
        return Ok(());
    }

    let ready = devices
        .iter()
        .filter(|d| d.state == "device" && d.is_tizen)
        .count();
    if ready == 0 {
        ctx.info("no ready Tizen devices found");
    } else {
        ctx.info(format!("found {ready} ready Tizen device(s):"));
    }

    let tag_ok = colorize(use_color, "1;32", "[ok]");
    let tag_warn = colorize(use_color, "1;33", "[warn]");

    let mut printed = 0usize;
    for device in &devices {
        let should_print = args.all || (device.state == "device" && device.is_tizen);
        if !should_print {
            continue;
        }

        if device.state != "device" {
            ctx.info(format!(
                "{} {} ({}) state={}",
                tag_warn, device.id, device.model, device.state
            ));
            printed += 1;
            continue;
        }

        if !device.is_tizen {
            ctx.info(format!(
                "{} {} ({}) is not a recognized Tizen target",
                tag_warn, device.id, device.model
            ));
            printed += 1;
            continue;
        }

        let profile = device.profile.as_deref().unwrap_or("unknown");
        let arch = device.cpu_arch.as_deref().unwrap_or("unknown");
        let secure = if device.secure_protocol {
            "enabled"
        } else {
            "disabled"
        };
        ctx.info(format!(
            "{} {} ({}) profile={} arch={} secure={}",
            tag_ok, device.id, device.model, profile, arch, secure
        ));
        printed += 1;
    }

    if printed == 0 && !args.all {
        ctx.info(
            "no ready Tizen devices. rerun with --all to inspect offline/unauthorized entries",
        );
    }

    Ok(())
}

pub fn resolve_target_device(ctx: &AppContext, requested_id: Option<&str>) -> Result<TizenDevice> {
    let devices = discover_devices(ctx)?;
    let ready: Vec<TizenDevice> = devices
        .iter()
        .filter(|d| d.state == "device" && d.is_tizen)
        .cloned()
        .collect();

    if let Some(id) = requested_id {
        if let Some(device) = ready.iter().find(|d| d.id == id) {
            return Ok(device.clone());
        }
        if let Some(device) = devices.iter().find(|d| d.id == id) {
            bail!(
                "device {} is present but not ready (state: {}). run `cargo tizen devices --all`",
                id,
                device.state
            );
        }
        let available = if ready.is_empty() {
            "<none>".to_string()
        } else {
            ready
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        bail!("device {} not found. ready devices: {}", id, available);
    }

    match ready.as_slice() {
        [single] => Ok(single.clone()),
        [] => bail!(
            "no ready Tizen devices found. connect a device (for example `sdb connect <ip:port>`) and rerun"
        ),
        many => {
            let list = many
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "multiple ready devices found: {}. use -d/--device <id>",
                list
            )
        }
    }
}

pub fn install_tpk_on_device(ctx: &AppContext, device: &TizenDevice, tpk: &Path) -> Result<()> {
    if !tpk.is_file() {
        bail!("tpk file does not exist: {}", tpk.display());
    }

    let sdb = locate_sdb(ctx)?;
    let mut cmd = Command::new(&sdb);
    cmd.arg("-s").arg(&device.id).arg("install").arg(tpk);

    let output = cmd
        .output()
        .with_context(|| format!("failed to execute sdb install for {}", device.id))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let lower = stdout.to_ascii_lowercase();

    if !output.status.success() || lower.contains("val[fail]") || lower.contains("install failed") {
        bail!(
            "failed to install {} on {}.\nstdout:\n{}\nstderr:\n{}",
            tpk.display(),
            device.id,
            stdout.trim(),
            stderr.trim()
        );
    }

    let use_color = color_enabled();
    ctx.info(format!(
        "{} {} on {}",
        cargo_status(use_color, "Installed"),
        tpk.display(),
        device.id
    ));
    Ok(())
}

pub fn discover_devices(ctx: &AppContext) -> Result<Vec<TizenDevice>> {
    let sdb = locate_sdb(ctx)?;
    let mut cmd = Command::new(&sdb);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);
    let output = cmd
        .arg("devices")
        .output()
        .context("failed to run `sdb devices`")?;
    if !output.status.success() {
        bail!(
            "`sdb devices` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_sdb_devices_output(&stdout);
    let mut devices = Vec::new();
    for entry in entries {
        let mut info = TizenDevice {
            id: entry.id.clone(),
            state: entry.state.clone(),
            model: entry.model.clone(),
            profile: None,
            cpu_arch: None,
            secure_protocol: false,
            is_tizen: false,
        };

        if entry.state == "device" {
            if let Ok(capabilities) = query_capabilities(ctx, &sdb, &entry.id) {
                if capabilities.contains_key("cpu_arch") {
                    info.profile = capabilities.get("profile_name").cloned();
                    info.cpu_arch = capabilities.get("cpu_arch").cloned();
                    info.secure_protocol = capabilities
                        .get("secure_protocol")
                        .is_some_and(|v| v == "enabled");
                    info.is_tizen = true;
                }
            }
        }

        devices.push(info);
    }

    Ok(devices)
}

fn locate_sdb(ctx: &AppContext) -> Result<PathBuf> {
    if let Some(sdk) = TizenSdk::locate(ctx.config.sdk_root().as_deref()) {
        let sdb = sdk.sdb();
        if sdb.is_file() {
            return Ok(sdb);
        }
    }

    if let Ok(found) = which::which("sdb") {
        return Ok(found);
    }

    bail!("unable to locate sdb. install Tizen SDK and ensure `sdb` is in PATH")
}

fn query_capabilities(
    ctx: &AppContext,
    sdb: &Path,
    device_id: &str,
) -> Result<HashMap<String, String>> {
    let mut cmd = Command::new(sdb);
    tool_env::tizen_cli_env(ctx).apply(&mut cmd);
    let output = cmd
        .arg("-s")
        .arg(device_id)
        .arg("capability")
        .output()
        .with_context(|| format!("failed to query capabilities for {}", device_id))?;
    if !output.status.success() {
        bail!(
            "capability query failed for {}: {}",
            device_id,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(map)
}

fn parse_sdb_devices_output(stdout: &str) -> Vec<SdbEntry> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("List of devices") {
            continue;
        }
        if trimmed.starts_with('*') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let id = parts[0].to_string();
        let state = parts[1].to_string();
        let model = if parts.len() >= 3 {
            parts[2..].join(" ")
        } else {
            "unknown".to_string()
        };

        entries.push(SdbEntry { id, state, model });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::parse_sdb_devices_output;

    #[test]
    fn parse_sdb_output_with_states() {
        let input = "\
List of devices attached
192.168.0.101:26101     device          SM-R800
0000d85900006200        offline         device-1
ABCDEF                  unauthorized    TV
";
        let rows = parse_sdb_devices_output(input);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "192.168.0.101:26101");
        assert_eq!(rows[0].state, "device");
        assert_eq!(rows[1].state, "offline");
        assert_eq!(rows[2].state, "unauthorized");
    }

    #[test]
    fn parse_sdb_empty_input() {
        let rows = parse_sdb_devices_output("");
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_sdb_header_only() {
        let rows = parse_sdb_devices_output("List of devices attached\n");
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_sdb_skips_star_lines() {
        let input = "List of devices attached\n* daemon started\n192.168.0.1:26101  device  TV\n";
        let rows = parse_sdb_devices_output(input);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "192.168.0.1:26101");
    }

    #[test]
    fn parse_sdb_two_part_line_defaults_model() {
        let input = "DEVICE_ID   offline\n";
        let rows = parse_sdb_devices_output(input);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "unknown");
    }

    #[test]
    fn parse_sdb_model_with_spaces() {
        let input = "192.168.0.1:26101   device   Samsung Galaxy Watch 4\n";
        let rows = parse_sdb_devices_output(input);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "Samsung Galaxy Watch 4");
    }
}
