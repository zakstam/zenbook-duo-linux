use crate::hardware::duo::{PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR};
use crate::ipc::protocol::SessionBackend;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PROC_INPUT_DEVICES: &str = "/proc/bus/input/devices";
const DRM_CLASS_DIR: &str = "/sys/class/drm";
const GNOME_TOUCH_DEVICE_SCHEMAS: [&str; 2] = ["touchscreens", "tablets"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct DuoTouchInput {
    device_id: String,
    connector: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GnomeOutputMapping {
    vendor: String,
    product: String,
    serial: String,
    connector: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DconfWrite {
    path: String,
    value: String,
}

pub(crate) fn apply_for_backend(backend: SessionBackend) -> Result<usize, String> {
    if backend != SessionBackend::Gnome {
        return Ok(0);
    }

    apply_gnome_touch_mappings()
}

fn apply_gnome_touch_mappings() -> Result<usize, String> {
    let proc_input = fs::read_to_string(PROC_INPUT_DEVICES)
        .map_err(|e| format!("Failed to read {PROC_INPUT_DEVICES}: {e}"))?;
    let devices = discover_duo_touch_inputs_from_proc_input(&proc_input);
    if devices.is_empty() {
        return Ok(0);
    }

    let outputs = read_internal_output_mappings()?;
    let writes = gnome_dconf_writes(&devices, &outputs);
    for write in &writes {
        run_dconf_write(write)?;
    }
    Ok(writes.len())
}

fn read_internal_output_mappings() -> Result<Vec<GnomeOutputMapping>, String> {
    let mut mappings = Vec::new();
    for connector in [PRIMARY_INTERNAL_CONNECTOR, SECONDARY_INTERNAL_CONNECTOR] {
        let Some(path) = edid_path_for_connector(connector) else {
            continue;
        };
        let edid =
            fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if let Some(mapping) = parse_edid_output(connector, &edid) {
            mappings.push(mapping);
        }
    }
    Ok(mappings)
}

fn edid_path_for_connector(connector: &str) -> Option<PathBuf> {
    let suffix = format!("-{connector}");
    let mut candidates = fs::read_dir(DRM_CLASS_DIR)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(&suffix) {
                Some(entry.path().join("edid"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().find(|path| path.exists())
}

fn parse_edid_output(connector: &str, edid: &[u8]) -> Option<GnomeOutputMapping> {
    if edid.len() < 16 || edid.get(0..8)? != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
        return None;
    }

    let manufacturer = u16::from_be_bytes([edid[8], edid[9]]);
    let vendor = edid_manufacturer_code(manufacturer)?;
    let product = u16::from_le_bytes([edid[10], edid[11]]);
    let serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);

    Some(GnomeOutputMapping {
        vendor,
        product: format!("0x{product:04x}"),
        serial: format!("0x{serial:08x}"),
        connector: connector.to_string(),
    })
}

fn edid_manufacturer_code(raw: u16) -> Option<String> {
    let chars = [
        ((raw >> 10) & 0x1f) as u8,
        ((raw >> 5) & 0x1f) as u8,
        (raw & 0x1f) as u8,
    ];
    if chars.iter().any(|value| !(1..=26).contains(value)) {
        return None;
    }

    Some(
        chars
            .into_iter()
            .map(|value| (b'A' + value - 1) as char)
            .collect(),
    )
}

fn discover_duo_touch_inputs_from_proc_input(input: &str) -> Vec<DuoTouchInput> {
    let mut seen = HashSet::new();
    let mut devices = Vec::new();

    for block in input.split("\n\n") {
        let Some((vendor, product)) = input_block_vendor_product(block) else {
            continue;
        };
        if !vendor.eq_ignore_ascii_case("04f3") {
            continue;
        }

        let name = input_block_name(block).unwrap_or_default();
        if name.contains("Touchpad") {
            continue;
        }

        let product = product.to_ascii_lowercase();
        let Some(connector) = connector_for_touch_device(&name, &product) else {
            continue;
        };
        let device_id = format!("{}:{}", vendor.to_ascii_lowercase(), product);
        if seen.insert(device_id.clone()) {
            devices.push(DuoTouchInput {
                device_id,
                connector: connector.to_string(),
            });
        }
    }

    devices.sort_by(|left, right| {
        connector_sort_key(&left.connector).cmp(&connector_sort_key(&right.connector))
    });
    devices
}

fn input_block_vendor_product(block: &str) -> Option<(String, String)> {
    let line = block
        .lines()
        .find(|line| line.trim_start().starts_with("I:"))?;
    let mut vendor = None;
    let mut product = None;
    for token in line.split_whitespace() {
        if let Some(value) = token.strip_prefix("Vendor=") {
            vendor = Some(value.to_string());
        } else if let Some(value) = token.strip_prefix("Product=") {
            product = Some(value.to_string());
        }
    }
    Some((vendor?, product?))
}

fn input_block_name(block: &str) -> Option<String> {
    let line = block
        .lines()
        .find(|line| line.trim_start().starts_with("N:"))?;
    let (_, value) = line.split_once("Name=")?;
    Some(value.trim().trim_matches('"').to_string())
}

fn connector_for_touch_device(name: &str, product: &str) -> Option<&'static str> {
    if name.contains("ELAN9008") || matches!(product, "425b" | "4447") {
        Some(PRIMARY_INTERNAL_CONNECTOR)
    } else if name.contains("ELAN9009") || matches!(product, "425a" | "4448") {
        Some(SECONDARY_INTERNAL_CONNECTOR)
    } else {
        None
    }
}

fn connector_sort_key(connector: &str) -> (u8, &str) {
    match connector {
        PRIMARY_INTERNAL_CONNECTOR => (0, connector),
        SECONDARY_INTERNAL_CONNECTOR => (1, connector),
        _ => (2, connector),
    }
}

fn gnome_dconf_writes(
    devices: &[DuoTouchInput],
    outputs: &[GnomeOutputMapping],
) -> Vec<DconfWrite> {
    let mut writes = Vec::new();
    for device in devices {
        let Some(output) = outputs
            .iter()
            .find(|mapping| mapping.connector == device.connector)
        else {
            continue;
        };

        for schema in GNOME_TOUCH_DEVICE_SCHEMAS {
            writes.push(DconfWrite {
                path: format!(
                    "/org/gnome/desktop/peripherals/{schema}/{}/output",
                    device.device_id
                ),
                value: output.dconf_value(),
            });
        }
    }
    writes
}

fn run_dconf_write(write: &DconfWrite) -> Result<(), String> {
    let output = Command::new("dconf")
        .args(["write", write.path.as_str(), write.value.as_str()])
        .output()
        .map_err(|e| format!("Failed to run dconf write {}: {e}", write.path))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("dconf write {} failed: {stderr}", write.path))
    }
}

impl GnomeOutputMapping {
    fn dconf_value(&self) -> String {
        format!(
            "['{}', '{}', '{}', '{}']",
            self.vendor, self.product, self.serial, self.connector
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_edid() -> Vec<u8> {
        let mut edid = vec![0_u8; 128];
        edid[0..16].copy_from_slice(&[
            0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x4c, 0x83, 0x9d, 0x41, 0x00, 0x00,
            0x00, 0x00,
        ]);
        edid
    }

    #[test]
    fn parses_gnome_edid_tuple_from_internal_panel() {
        let mapping = parse_edid_output(PRIMARY_INTERNAL_CONNECTOR, &sample_edid())
            .expect("EDID should parse");

        assert_eq!(mapping.vendor, "SDC");
        assert_eq!(mapping.product, "0x419d");
        assert_eq!(mapping.serial, "0x00000000");
        assert_eq!(mapping.connector, PRIMARY_INTERNAL_CONNECTOR);
        assert_eq!(
            mapping.dconf_value(),
            "['SDC', '0x419d', '0x00000000', 'eDP-1']"
        );
    }

    #[test]
    fn discovers_duo_touch_inputs_and_deduplicates_stylus_siblings() {
        let proc_input = r#"
I: Bus=0018 Vendor=04f3 Product=4447 Version=0100
N: Name="ELAN9008:00 04F3:4447"
P: Phys=i2c-ELAN9008:00
H: Handlers=event16 mouse5

I: Bus=0018 Vendor=04f3 Product=4447 Version=0100
N: Name="ELAN9008:00 04F3:4447 Stylus"
P: Phys=i2c-ELAN9008:00
H: Handlers=event19 mouse6

I: Bus=0018 Vendor=04f3 Product=4447 Version=0100
N: Name="ELAN9008:00 04F3:4447 Touchpad"
P: Phys=i2c-ELAN9008:00
H: Handlers=event20 mouse7

I: Bus=0018 Vendor=04f3 Product=4448 Version=0100
N: Name="ELAN9009:00 04F3:4448"
P: Phys=i2c-ELAN9009:00
H: Handlers=event11 mouse2
"#;

        assert_eq!(
            discover_duo_touch_inputs_from_proc_input(proc_input),
            vec![
                DuoTouchInput {
                    device_id: "04f3:4447".into(),
                    connector: PRIMARY_INTERNAL_CONNECTOR.into(),
                },
                DuoTouchInput {
                    device_id: "04f3:4448".into(),
                    connector: SECONDARY_INTERNAL_CONNECTOR.into(),
                },
            ]
        );
    }

    #[test]
    fn generates_touchscreen_and_tablet_dconf_writes() {
        let devices = vec![
            DuoTouchInput {
                device_id: "04f3:4447".into(),
                connector: PRIMARY_INTERNAL_CONNECTOR.into(),
            },
            DuoTouchInput {
                device_id: "04f3:4448".into(),
                connector: SECONDARY_INTERNAL_CONNECTOR.into(),
            },
        ];
        let outputs = vec![
            GnomeOutputMapping {
                vendor: "SDC".into(),
                product: "0x419d".into(),
                serial: "0x00000000".into(),
                connector: PRIMARY_INTERNAL_CONNECTOR.into(),
            },
            GnomeOutputMapping {
                vendor: "SDC".into(),
                product: "0x419d".into(),
                serial: "0x00000000".into(),
                connector: SECONDARY_INTERNAL_CONNECTOR.into(),
            },
        ];

        assert_eq!(
            gnome_dconf_writes(&devices, &outputs),
            vec![
                DconfWrite {
                    path: "/org/gnome/desktop/peripherals/touchscreens/04f3:4447/output".into(),
                    value: "['SDC', '0x419d', '0x00000000', 'eDP-1']".into(),
                },
                DconfWrite {
                    path: "/org/gnome/desktop/peripherals/tablets/04f3:4447/output".into(),
                    value: "['SDC', '0x419d', '0x00000000', 'eDP-1']".into(),
                },
                DconfWrite {
                    path: "/org/gnome/desktop/peripherals/touchscreens/04f3:4448/output".into(),
                    value: "['SDC', '0x419d', '0x00000000', 'eDP-2']".into(),
                },
                DconfWrite {
                    path: "/org/gnome/desktop/peripherals/tablets/04f3:4448/output".into(),
                    value: "['SDC', '0x419d', '0x00000000', 'eDP-2']".into(),
                },
            ]
        );
    }
}
