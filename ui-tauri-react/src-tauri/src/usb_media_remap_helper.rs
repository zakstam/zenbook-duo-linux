use chrono::Local;
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, Device, EventType, InputEvent, Key};
use nix::fcntl::{fcntl, FcntlArg, Flock, FlockArg, OFlag};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use signal_hook::flag;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::runtime::paths;

pub fn run_from_env() -> Result<(), String> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(args: I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let args = parse_args(args);
    let base_dir = base_dir_from_pid_file(&args.pid_file);

    if args.stop {
        return stop_process(&args.pid_file);
    }

    ensure_dir(&base_dir)?;

    let device_path = args
        .device
        .clone()
        .or_else(find_keyboard_device)
        .ok_or_else(|| "USB keyboard event device not found".to_string())?;

    log_info(&format!(
        "Starting helper with keyboard device {}",
        device_path.display()
    ));

    let mut device = Device::open(&device_path)
        .map_err(|e| format!("Failed to open {}: {e}", device_path.display()))?;
    device.grab().map_err(|e| {
        format!(
            "Failed to grab device {}: {}. Another process may already hold an exclusive grab.",
            device_path.display(),
            e
        )
    })?;
    configure_nonblocking(&device).map_err(|e| {
        format!(
            "Failed to configure {} as non-blocking: {e}",
            device_path.display()
        )
    })?;
    log_info(&format!(
        "Grabbed keyboard device {} successfully",
        device_path.display()
    ));

    let mut keys = AttributeSet::<Key>::new();
    if let Some(supported) = device.supported_keys() {
        for key in supported.iter() {
            keys.insert(key);
        }
    }

    for key in [
        Key::KEY_MUTE,
        Key::KEY_VOLUMEDOWN,
        Key::KEY_VOLUMEUP,
        Key::KEY_BRIGHTNESSDOWN,
        Key::KEY_BRIGHTNESSUP,
    ] {
        keys.insert(key);
    }

    let mut uinput = VirtualDeviceBuilder::new()
        .map_err(|e| format!("Failed to init uinput builder: {e}"))?
        .name("Zenbook Duo USB Remap")
        .with_keys(&keys)
        .map_err(|e| format!("Failed to set keys for uinput: {e}"))?
        .build()
        .map_err(|e| format!("Failed to create uinput device: {e}"))?;

    write_pid(&args.pid_file)?;
    let _pid_guard = PidFileGuard::new(args.pid_file.clone());

    let terminate = Arc::new(AtomicBool::new(false));
    flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminate))
        .map_err(|e| format!("Failed to register SIGTERM handler: {e}"))?;
    flag::register(signal_hook::consts::SIGINT, Arc::clone(&terminate))
        .map_err(|e| format!("Failed to register SIGINT handler: {e}"))?;

    let pause_file = base_dir.join("usb_media_remap.paused");

    while !terminate.load(Ordering::Relaxed) {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Failed to read events: {e}")),
        };
        let paused = pause_file.exists();
        for event in events {
            if terminate.load(Ordering::Relaxed) {
                break;
            }
            if paused {
                // Pass through all KEY events without remapping.
                if event.event_type() == EventType::KEY {
                    emit_key(&mut uinput, Key::new(event.code()), event.value())?;
                }
            } else {
                handle_event(&mut uinput, &args, event)?;
            }
        }
    }

    let _ = device.ungrab();
    log_info("Stopping helper");
    Ok(())
}

pub fn log_error(message: &str) {
    eprintln!("USB-REMAP - ERROR: {}", message);
    log_line("ERROR", message);
}

pub fn log_info(message: &str) {
    eprintln!("USB-REMAP - INFO: {}", message);
    log_line("INFO", message);
}

fn log_line(level: &str, message: &str) {
    // Best-effort: derive the log location from --pid-file (or default).
    let pid_file = pid_file_from_env_args();
    let base_dir = base_dir_from_pid_file(&pid_file);
    let _ = ensure_dir(&base_dir);
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_path = base_dir.join("duo.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{} - USB-REMAP - {}: {}", timestamp, level, message);
    }
}

#[derive(Debug, Clone)]
struct Args {
    pid_file: String,
    user: Option<String>,
    device: Option<PathBuf>,
    stop: bool,
}

fn parse_args<I>(mut args: I) -> Args
where
    I: Iterator<Item = String>,
{
    let mut pid_file = default_pid_file();
    let mut user = None;
    let mut device = None;
    let mut stop = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pid-file" => {
                if let Some(value) = args.next() {
                    pid_file = value;
                }
            }
            "--user" => {
                if let Some(value) = args.next() {
                    user = Some(value);
                }
            }
            "--device" => {
                if let Some(value) = args.next() {
                    device = Some(PathBuf::from(value));
                }
            }
            "--stop" => stop = true,
            _ => {}
        }
    }

    Args {
        pid_file,
        user,
        device,
        stop,
    }
}

fn handle_event(
    uinput: &mut evdev::uinput::VirtualDevice,
    args: &Args,
    event: InputEvent,
) -> Result<(), String> {
    if event.event_type() != EventType::KEY {
        return Ok(());
    }

    let key = Key::new(event.code());
    let value = event.value();

    match key {
        Key::KEY_F4 => {
            if value == 1 {
                cycle_backlight();
            }
            return Ok(());
        }
        Key::KEY_F5 => {
            if value == 1 {
                step_brightness("down")?;
            }
            return Ok(());
        }
        Key::KEY_F6 => {
            if value == 1 {
                step_brightness("up")?;
            }
            return Ok(());
        }
        Key::KEY_F11 => {
            if value == 1 {
                open_emoji_picker(args.user.as_deref());
            }
            return Ok(());
        }
        _ => {}
    }

    let mapped = match key {
        Key::KEY_F1 => Key::KEY_MUTE,
        Key::KEY_F2 => Key::KEY_VOLUMEDOWN,
        Key::KEY_F3 => Key::KEY_VOLUMEUP,
        _ => key,
    };

    emit_key(uinput, mapped, value)
}

fn emit_key(uinput: &mut evdev::uinput::VirtualDevice, key: Key, value: i32) -> Result<(), String> {
    let events = [
        InputEvent::new(EventType::KEY, key.code(), value),
        InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
    ];
    uinput
        .emit(&events)
        .map_err(|e| format!("Failed to emit key event: {e}"))
}

fn write_pid(path: &str) -> Result<(), String> {
    if let Ok(existing) = fs::read_to_string(path) {
        if let Ok(pid) = existing.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                return Err(format!("Remapper already running (pid {})", pid));
            }
        }
    }
    fs::write(path, std::process::id().to_string())
        .map_err(|e| format!("Failed to write pid file: {e}"))
}

fn configure_nonblocking(device: &Device) -> Result<(), nix::Error> {
    let fd = device.as_raw_fd();
    let current_flags = OFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFL)?);
    fcntl(fd, FcntlArg::F_SETFL(current_flags | OFlag::O_NONBLOCK))?;
    Ok(())
}

struct PidFileGuard {
    path: String,
}

impl PidFileGuard {
    fn new(path: String) -> Self {
        Self { path }
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn stop_process(path: &str) -> Result<(), String> {
    let pid = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read pid file: {e}"))?
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Invalid pid file contents: {e}"))?;

    let pid = Pid::from_raw(pid);
    let _ = kill(pid, Signal::SIGTERM);

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let running = unsafe { libc::kill(pid.as_raw(), 0) == 0 };
        if !running {
            let _ = fs::remove_file(path);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = kill(pid, Signal::SIGKILL);
    let _ = fs::remove_file(path);
    Ok(())
}

fn find_keyboard_device() -> Option<PathBuf> {
    let by_id = Path::new("/dev/input/by-id");
    let entries = fs::read_dir(by_id).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.contains("Zenbook_Duo_Keyboard") && name.contains("event-kbd") {
            return Some(entry.path());
        }
        if name.contains("ASUS_Zenbook_Duo_Keyboard") && name.contains("event-kbd") {
            return Some(entry.path());
        }
    }
    None
}

fn cycle_backlight() {
    // Backlight state is shared with the main app via files next to the pid file.
    let pid_file = pid_file_from_env_args();
    let base_dir = base_dir_from_pid_file(&pid_file);
    if ensure_dir(&base_dir).is_err() {
        return;
    }

    let kbl_lock_path = base_dir.join("kb_backlight_lock");
    let kbl_level_path = base_dir.join("kb_backlight_level");
    let kbl_last_cycle_path = base_dir.join("kb_backlight_last_cycle");

    let lock = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(kbl_lock_path)
    {
        Ok(file) => file,
        Err(_) => return,
    };

    let _lock = match Flock::lock(lock, FlockArg::LockExclusiveNonblock) {
        Ok(l) => l,
        Err(_) => return,
    };

    let now = current_time_ms();
    if let Some(last) = fs::read_to_string(&kbl_last_cycle_path)
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
    {
        if now.saturating_sub(last) < 600 {
            return;
        }
    }

    let _ = fs::write(&kbl_last_cycle_path, now.to_string());

    let level = fs::read_to_string(&kbl_level_path)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0);
    let next = (level + 1) % 4;
    if crate::commands::backlight::set_backlight_daemon_first(next).is_ok() {
        let _ = fs::write(&kbl_level_path, next.to_string());
    }
}

fn open_emoji_picker(user: Option<&str>) {
    let user = match user {
        Some(u) => u,
        None => return,
    };

    let uid = match nix::unistd::User::from_name(user) {
        Ok(Some(u)) => u.uid.as_raw(),
        _ => return,
    };

    let is_running = Command::new("pgrep")
        .arg("-u")
        .arg(uid.to_string())
        .arg("-x")
        .arg("gnome-characters")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if is_running {
        return;
    }

    let runtime_dir = format!("/run/user/{uid}");
    let bus_address = format!("unix:path={runtime_dir}/bus");

    let mut cmd = if nix::unistd::Uid::current().is_root() {
        let mut cmd = Command::new("runuser");
        cmd.arg("-u").arg(user).arg("--").arg("env");
        cmd
    } else {
        Command::new("env")
    };

    cmd.arg(format!("XDG_RUNTIME_DIR={runtime_dir}"))
        .arg(format!("DBUS_SESSION_BUS_ADDRESS={bus_address}"));

    if Path::new(&format!("{runtime_dir}/wayland-0")).exists() {
        cmd.arg("WAYLAND_DISPLAY=wayland-0");
    } else if Path::new("/tmp/.X11-unix/X0").exists() {
        cmd.arg("DISPLAY=:0");
    }

    let _ = cmd.arg("gnome-characters").spawn();
}

fn current_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

fn step_brightness(direction: &str) -> Result<(), String> {
    let primary = Path::new("/sys/class/backlight/intel_backlight");
    if !primary.exists() {
        return Err("no intel_backlight device found".into());
    }

    let primary_max = read_backlight_value(&primary.join("max_brightness"))?;
    let current = read_backlight_value(&primary.join("brightness"))?;
    let next = next_brightness_value(current, primary_max, direction);

    fs::write(primary.join("brightness"), next.to_string())
        .map_err(|e| format!("Failed to write primary brightness: {e}"))?;

    let secondary = Path::new("/sys/class/backlight/card1-eDP-2-backlight");
    if secondary.exists() {
        let secondary_max = read_backlight_value(&secondary.join("max_brightness"))?;
        let mirrored = next.min(secondary_max);
        fs::write(secondary.join("brightness"), mirrored.to_string())
            .map_err(|e| format!("Failed to write secondary brightness: {e}"))?;
    }

    Ok(())
}

fn read_backlight_value(path: &Path) -> Result<i32, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Invalid brightness value in {}: {e}", path.display()))
}

fn next_brightness_value(current: i32, max: i32, direction: &str) -> i32 {
    let step = (max / 20).max(1);
    if direction == "up" {
        (current + step).min(max)
    } else {
        (current - step).max(0)
    }
}

fn default_pid_file() -> String {
    paths::current_user_runtime_dir()
        .join("usb_media_remap.pid")
        .to_string_lossy()
        .into_owned()
}

fn pid_file_from_env_args() -> String {
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        if arg == "--pid-file" {
            if let Some(v) = it.next() {
                return v;
            }
        }
    }
    default_pid_file()
}

fn base_dir_from_pid_file(pid_file: &str) -> PathBuf {
    Path::new(pid_file)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(paths::current_user_runtime_dir)
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    crate::runtime::runtime_dir::ensure_dir_owned_like_parent(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_step_uses_five_percent_chunks() {
        assert_eq!(next_brightness_value(200, 400, "up"), 220);
        assert_eq!(next_brightness_value(200, 400, "down"), 180);
        assert_eq!(next_brightness_value(395, 400, "up"), 400);
        assert_eq!(next_brightness_value(5, 400, "down"), 0);
    }
}
