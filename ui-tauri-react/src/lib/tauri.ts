import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DuoStatus,
  DuoSettings,
  DisplayLayout,
  Orientation,
  Profile,
  HardwareEvent,
  EvdevDevice,
  EvdevEvent,
  EvdevEventMulti,
  HidDevice,
  ReportDescriptor,
  HidrawCapture,
  UsbMediaRemapStatus,
} from "@/types/duo";

// Status
export const getStatus = () => invoke<DuoStatus>("get_status");

// Backlight
export const getBacklight = () => invoke<number>("get_backlight");
export const setBacklight = (level: number) =>
  invoke<void>("set_backlight", { level });

// Display
export const getDisplayLayout = () =>
  invoke<DisplayLayout>("get_display_layout");
export const applyDisplayLayout = (layout: DisplayLayout) =>
  invoke<void>("apply_display_layout", { layout });
export const setOrientation = (orientation: Orientation) =>
  invoke<void>("set_orientation", { orientation });

// Service
export const isServiceActive = () => invoke<boolean>("is_service_active");
export const restartService = () => invoke<void>("restart_service");

// Settings
export const loadSettings = () => invoke<DuoSettings>("load_settings");
export const saveSettings = (settings: DuoSettings) =>
  invoke<void>("save_settings", { settings });

// Logs
export const readLog = (lines: number) =>
  invoke<string[]>("read_log", { lines });
export const clearLog = () => invoke<void>("clear_log");

// Profiles
export const listProfiles = () => invoke<Profile[]>("list_profiles");
export const saveProfile = (profile: Profile) =>
  invoke<void>("save_profile", { profile });
export const deleteProfile = (id: string) =>
  invoke<void>("delete_profile", { id });
export const activateProfile = (id: string) =>
  invoke<void>("activate_profile", { id });

// Events
export const getRecentEvents = (count: number) =>
  invoke<HardwareEvent[]>("get_recent_events", { count });

// Diagnostics
export const diagListEvdev = () => invoke<EvdevDevice[]>("diag_list_evdev");
export const diagCaptureEvdev = (eventPath: string, seconds: number) =>
  invoke<EvdevEvent[]>("diag_capture_evdev", { eventPath, seconds });
export const diagCaptureEvdevMulti = (eventPaths: string[], seconds: number) =>
  invoke<EvdevEventMulti[]>("diag_capture_evdev_multi", { eventPaths, seconds });
export const diagListHid = (vid: string, pid: string) =>
  invoke<HidDevice[]>("diag_list_hid", { vid, pid });
export const diagReadReportDescriptor = (hidDeviceId: string) =>
  invoke<ReportDescriptor>("diag_read_report_descriptor", { hidDeviceId });
export const diagCaptureHidrawPkexec = (hidrawPath: string, seconds: number) =>
  invoke<HidrawCapture>("diag_capture_hidraw_pkexec", { hidrawPath, seconds });

// USB media remap
export const usbMediaRemapStatus = () =>
  invoke<UsbMediaRemapStatus>("usb_media_remap_status");
export const usbMediaRemapStart = () => invoke<void>("usb_media_remap_start");
export const usbMediaRemapStop = () => invoke<void>("usb_media_remap_stop");


// Tauri event listeners
export const onStatusChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("duo://status-changed", cb);

export const onLogUpdated = (cb: () => void): Promise<UnlistenFn> =>
  listen("duo://log-updated", cb);

export const onKeyboardChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("duo://keyboard-changed", cb);

export const onHardwareEvent = (cb: () => void): Promise<UnlistenFn> =>
  listen("duo://hardware-event", cb);
