use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvdevDevice {
    pub event_path: String,
    pub name: String,
    pub phys: Option<String>,
    pub bustype: Option<String>,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub cap_ev: Option<String>,
    pub cap_key: Option<String>,
    pub cap_abs: Option<String>,
    pub cap_msc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvdevEvent {
    pub ts_sec: i64,
    pub ts_usec: i64,
    pub type_code: u16,
    pub code: u16,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvdevEventMulti {
    pub event_path: String,
    pub ts_sec: i64,
    pub ts_usec: i64,
    pub type_code: u16,
    pub code: u16,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidDevice {
    pub id: String,
    pub driver: Option<String>,
    pub hid_id: Option<String>,
    pub hid_name: Option<String>,
    pub hid_phys: Option<String>,
    pub hidraw_nodes: Vec<String>,
    pub input_event_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDescriptor {
    pub len: usize,
    pub hex: String,
    pub report_ids: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidrawSample {
    pub ts_ms: u128,
    pub hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidrawCapture {
    pub hidraw_path: String,
    pub samples: Vec<HidrawSample>,
    pub stderr: Option<String>,
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn is_event_node_name(name: &str) -> bool {
    if !name.starts_with("event") {
        return false;
    }
    name["event".len()..].chars().all(|c| c.is_ascii_digit())
}

#[tauri::command]
pub fn diag_list_evdev() -> Result<Vec<EvdevDevice>, String> {
    let base = Path::new("/sys/class/input");
    let mut out: Vec<EvdevDevice> = Vec::new();
    let entries =
        fs::read_dir(base).map_err(|e| format!("Failed to read /sys/class/input: {e}"))?;

    for ent in entries.flatten() {
        let file_name = ent.file_name();
        let file_name = file_name.to_string_lossy().to_string();
        if !is_event_node_name(&file_name) {
            continue;
        }

        let sys = ent.path();
        let dev = sys.join("device");

        let name = read_trimmed(dev.join("name")).unwrap_or_else(|| "(unknown)".into());
        let phys = read_trimmed(dev.join("phys"));

        let bustype = read_trimmed(dev.join("id/bustype"));
        let vendor = read_trimmed(dev.join("id/vendor"));
        let product = read_trimmed(dev.join("id/product"));

        let cap_ev = read_trimmed(dev.join("capabilities/ev"));
        let cap_key = read_trimmed(dev.join("capabilities/key"));
        let cap_abs = read_trimmed(dev.join("capabilities/abs"));
        let cap_msc = read_trimmed(dev.join("capabilities/msc"));

        out.push(EvdevDevice {
            event_path: format!("/dev/input/{file_name}"),
            name,
            phys,
            bustype,
            vendor,
            product,
            cap_ev,
            cap_key,
            cap_abs,
            cap_msc,
        });
    }

    out.sort_by(|a, b| a.event_path.cmp(&b.event_path));
    Ok(out)
}

fn validate_dev_input_event(path: &str) -> Result<(), String> {
    if !path.starts_with("/dev/input/event") {
        return Err("Only /dev/input/event* paths are allowed".into());
    }
    let suffix = &path["/dev/input/event".len()..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid event node path".into());
    }
    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

#[tauri::command]
pub fn diag_capture_evdev(event_path: String, seconds: u32) -> Result<Vec<EvdevEvent>, String> {
    validate_dev_input_event(&event_path)?;
    let seconds = seconds.clamp(1, 30);

    let file =
        fs::File::open(&event_path).map_err(|e| format!("Failed to open {event_path}: {e}"))?;
    let fd = file.as_raw_fd();

    // Non-blocking reads.
    let flags = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFL)
        .map_err(|e| format!("fcntl(F_GETFL) failed: {e}"))?;
    let mut oflags = nix::fcntl::OFlag::from_bits_truncate(flags);
    oflags.insert(nix::fcntl::OFlag::O_NONBLOCK);
    nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFL(oflags))
        .map_err(|e| format!("fcntl(F_SETFL) failed: {e}"))?;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds as u64);
    let mut out: Vec<EvdevEvent> = Vec::new();
    let mut buf = vec![0u8; 4096];

    while Instant::now() < deadline {
        match nix::unistd::read(fd, &mut buf) {
            Ok(0) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(n) => {
                let mut offset = 0usize;
                while offset + std::mem::size_of::<InputEvent>() <= n {
                    let ptr = unsafe { buf.as_ptr().add(offset) as *const InputEvent };
                    let ev = unsafe { *ptr };
                    out.push(EvdevEvent {
                        ts_sec: ev.time.tv_sec as i64,
                        ts_usec: ev.time.tv_usec as i64,
                        type_code: ev.type_,
                        code: ev.code,
                        value: ev.value,
                    });
                    offset += std::mem::size_of::<InputEvent>();
                }
            }
            Err(err) => {
                if err == nix::errno::Errno::EAGAIN {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                return Err(format!("Failed to read {event_path}: {err}"));
            }
        }
    }

    Ok(out)
}

#[tauri::command]
pub fn diag_capture_evdev_multi(
    event_paths: Vec<String>,
    seconds: u32,
) -> Result<Vec<EvdevEventMulti>, String> {
    if event_paths.is_empty() {
        return Ok(Vec::new());
    }
    let seconds = seconds.clamp(1, 30);
    for p in &event_paths {
        validate_dev_input_event(p)?;
    }

    let deadline = Instant::now() + Duration::from_secs(seconds as u64);
    let out: Arc<Mutex<Vec<EvdevEventMulti>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for path in event_paths {
        let out = out.clone();
        handles.push(std::thread::spawn(move || {
            let file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => return,
            };
            let fd = file.as_raw_fd();

            let flags = match nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFL) {
                Ok(v) => v,
                Err(_) => return,
            };
            let mut oflags = nix::fcntl::OFlag::from_bits_truncate(flags);
            oflags.insert(nix::fcntl::OFlag::O_NONBLOCK);
            let _ = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFL(oflags));

            let mut buf = vec![0u8; 4096];
            while Instant::now() < deadline {
                match nix::unistd::read(fd, &mut buf) {
                    Ok(0) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(n) => {
                        let mut offset = 0usize;
                        while offset + std::mem::size_of::<InputEvent>() <= n {
                            let ptr = unsafe { buf.as_ptr().add(offset) as *const InputEvent };
                            let ev = unsafe { *ptr };
                            let mut guard = out.lock().unwrap();
                            guard.push(EvdevEventMulti {
                                event_path: path.clone(),
                                ts_sec: ev.time.tv_sec as i64,
                                ts_usec: ev.time.tv_usec as i64,
                                type_code: ev.type_,
                                code: ev.code,
                                value: ev.value,
                            });
                            drop(guard);
                            offset += std::mem::size_of::<InputEvent>();
                        }
                    }
                    Err(err) => {
                        if err == nix::errno::Errno::EAGAIN {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        return;
                    }
                }
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    let mut v = out.lock().unwrap().clone();
    v.sort_by(|a, b| {
        (a.ts_sec, a.ts_usec, a.event_path.clone()).cmp(&(
            b.ts_sec,
            b.ts_usec,
            b.event_path.clone(),
        ))
    });
    Ok(v)
}

fn parse_hid_uevent(contents: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in contents.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn parse_hid_id(hid_id: &str) -> Option<(String, String, String)> {
    // HID_ID=0003:00000B05:00001B2C
    let parts: Vec<&str> = hid_id.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    ))
}

fn collect_hidraw_nodes(dev_path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let hidraw_dir = dev_path.join("hidraw");
    if let Ok(entries) = fs::read_dir(hidraw_dir) {
        for ent in entries.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with("hidraw") {
                out.push(format!("/dev/{name}"));
            }
        }
    }
    out.sort();
    out
}

fn collect_input_event_nodes(dev_path: &Path) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    let input_dir = dev_path.join("input");
    let Ok(entries) = fs::read_dir(input_dir) else {
        return Vec::new();
    };

    for ent in entries.flatten() {
        let p = ent.path();
        // /sys/bus/hid/devices/.../input/inputNN
        if let Ok(sub) = fs::read_dir(&p) {
            for sub_ent in sub.flatten() {
                let name = sub_ent.file_name().to_string_lossy().to_string();
                if is_event_node_name(&name) {
                    out.insert(format!("/dev/input/{name}"));
                }
            }
        }
    }

    out.into_iter().collect()
}

#[tauri::command]
pub fn diag_list_hid(vid: String, pid: String) -> Result<Vec<HidDevice>, String> {
    let base = Path::new("/sys/bus/hid/devices");
    let entries =
        fs::read_dir(base).map_err(|e| format!("Failed to read /sys/bus/hid/devices: {e}"))?;

    let want_vid = vid.trim().to_ascii_lowercase();
    let want_pid = pid.trim().to_ascii_lowercase();

    let mut out: Vec<HidDevice> = Vec::new();
    for ent in entries.flatten() {
        let id = ent.file_name().to_string_lossy().to_string();
        let dev_path = ent.path();
        let uevent = match fs::read_to_string(dev_path.join("uevent")) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let map = parse_hid_uevent(&uevent);
        let hid_id = map.get("HID_ID").cloned();
        let Some(hid_id_val) = hid_id.clone() else {
            continue;
        };
        let Some((_bus, v, p)) = parse_hid_id(&hid_id_val) else {
            continue;
        };

        if v.trim_start_matches('0').to_ascii_lowercase() != want_vid.trim_start_matches('0')
            || p.trim_start_matches('0').to_ascii_lowercase() != want_pid.trim_start_matches('0')
        {
            continue;
        }

        out.push(HidDevice {
            id,
            driver: map.get("DRIVER").cloned(),
            hid_id,
            hid_name: map.get("HID_NAME").cloned(),
            hid_phys: map.get("HID_PHYS").cloned(),
            hidraw_nodes: collect_hidraw_nodes(&dev_path),
            input_event_nodes: collect_input_event_nodes(&dev_path),
        });
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn validate_hid_device_id(id: &str) -> Result<(), String> {
    // Prevent path traversal; allow only sane sysfs names.
    if id.is_empty() {
        return Err("Empty HID device id".into());
    }
    if id.contains('/') || id.contains("..") {
        return Err("Invalid HID device id".into());
    }
    Ok(())
}

#[tauri::command]
pub fn diag_read_report_descriptor(hid_device_id: String) -> Result<ReportDescriptor, String> {
    validate_hid_device_id(&hid_device_id)?;
    let path = PathBuf::from("/sys/bus/hid/devices")
        .join(&hid_device_id)
        .join("report_descriptor");
    let bytes = fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    // Trim trailing zeros (sysfs often exposes a 4096-byte padded blob).
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    let bytes = &bytes[..end];

    let mut ids: BTreeSet<u8> = BTreeSet::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x85 {
            ids.insert(bytes[i + 1]);
            i += 2;
            continue;
        }
        i += 1;
    }

    Ok(ReportDescriptor {
        len: bytes.len(),
        hex: bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        report_ids: ids.into_iter().collect(),
    })
}

fn validate_hidraw(path: &str) -> Result<(), String> {
    if !path.starts_with("/dev/hidraw") {
        return Err("Only /dev/hidraw* paths are allowed".into());
    }
    let suffix = &path["/dev/hidraw".len()..];
    if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid hidraw path".into());
    }
    Ok(())
}

#[tauri::command]
pub fn diag_capture_hidraw_pkexec(
    hidraw_path: String,
    seconds: u32,
) -> Result<HidrawCapture, String> {
    validate_hidraw(&hidraw_path)?;
    let seconds = seconds.clamp(1, 15);

    // Use pkexec to run a bounded python reader for root-only hidraw nodes.
    let script = r#"
import sys, os, select, time, json, binascii, errno

path = sys.argv[1]
seconds = float(sys.argv[2])

fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
start = time.time()
last = None

while time.time() - start < seconds:
    r, _, _ = select.select([fd], [], [], 0.2)
    if not r:
        continue
    try:
        data = os.read(fd, 64)
    except OSError as e:
        if e.errno in (errno.EIO, errno.EAGAIN):
            continue
        raise
    if not data:
        continue
    if data != last:
        out = {"tsMs": int((time.time() - start) * 1000), "hex": binascii.hexlify(data).decode()}
        print(json.dumps(out), flush=True)
        last = data

os.close(fd)
"#;

    let mut child = Command::new("pkexec")
        .arg("/usr/bin/python3")
        .arg("-")
        .arg(&hidraw_path)
        .arg(seconds.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pkexec/python3: {e}"))?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to open stdin for pkexec child".to_string())?;
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| format!("Failed to write python script: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for pkexec: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut samples: Vec<HidrawSample> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<HidrawSample>(line) {
            Ok(s) => samples.push(s),
            Err(_) => {
                // Ignore parse errors; keep going.
            }
        }
    }

    Ok(HidrawCapture {
        hidraw_path,
        samples,
        stderr: if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        },
    })
}
