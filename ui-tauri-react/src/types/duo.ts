export type ConnectionType = "usb" | "bluetooth" | "none";
export type Orientation = "normal" | "left" | "right" | "inverted";
export type RefreshPolicy = "fixed" | "dynamic";
export type EventCategory = "USB" | "DISPLAY" | "KEYBOARD" | "NETWORK" | "ROTATION" | "BLUETOOTH" | "SERVICE";
export type EventSeverity = "info" | "warning" | "error";
export type ThemePreference = "system" | "light" | "dark";

export interface DuoStatus {
  keyboardAttached: boolean;
  connectionType: ConnectionType;
  monitorCount: number;
  wifiEnabled: boolean;
  bluetoothEnabled: boolean;
  backlightLevel: number;
  displayBrightness: number;
  maxBrightness: number;
  serviceActive: boolean;
  orientation: Orientation;
  /** Top row sends F1-F12 by default (toggled with Fn+Esc). */
  fnLock: boolean;
  /** The detachable keyboard's battery percentage, when it reports one. */
  keyboardBattery: number | null;
  /** Whether the default microphone is muted (the mic-mute key's LED), once known. */
  micMuted: boolean | null;
  /** The keyboard has its own Fn lock (the UX8406MA's): Fn+Esc, the Fn-lock switch, the mic-volume chord. */
  hardwareFnLock: boolean;
  /** The desktop the session agent runs in, while it is connected. */
  desktop: Desktop | null;
}

export type Desktop = "gnome" | "kde" | "hyprland" | "niri" | "unknown";

export interface DisplayInfo {
  connector: string;
  width: number;
  height: number;
  refreshRate: number;
  scale: number;
  x: number;
  y: number;
  transform: number;
  primary: boolean;
  currentMode: DisplayMode;
  availableModes: DisplayMode[];
  refreshPolicy: RefreshPolicy;
  supportsDynamicRefresh: boolean;
}

export interface DisplayLayout {
  displays: DisplayInfo[];
}

export interface DisplayMode {
  modeId: string;
  backendModeId?: string | null;
  width: number;
  height: number;
  refreshRate: number;
}

export interface DuoSettings {
  defaultBacklight: number;
  defaultScale: number;
  autoDualScreen: boolean;
  syncBrightness: boolean;
  theme: ThemePreference;
  usbMediaRemapEnabled: boolean;
  startOnBootMinimized: boolean;
  invertSensorRotation: boolean;
  setupCompleted: boolean;
  touchscreenDisabled: string[];
  savedDisplayLayout: DisplayLayout | null;
  /** Keyboard backlight off after this many seconds without input; 0 = never. */
  keyboardBacklightIdleSecs: number;
  /** What the top-row keys do in place of their ASUS function, by key ("F1"–"F12"). */
  keyOverrides: Record<string, KeyOverride>;
}

/** A top-row key's replacement for its ASUS function; the plain F-key (Fn) never changes. */
export type KeyOverride =
  | { action: "default" }
  | { action: "fKey" }
  | { action: "command"; command: string }
  | { action: "keys"; keys: string }
  | { action: "shortcut"; signal: string }
  | { action: "nothing" };

export interface TouchscreenDevice {
  name: string;
  i2cId: string;
  connector: string;
  enabled: boolean;
}

export interface UsbMediaRemapStatus {
  running: boolean;
  pid?: number | null;
  paused: boolean;
}

export interface VersionInfo {
  appVersion: string;
  appProtocolVersion: number;
  daemonVersion?: string | null;
  daemonProtocolVersion?: number | null;
  serviceAvailable: boolean;
}

export interface Profile {
  id: string;
  name: string;
  backlightLevel: number;
  scale: number;
  orientation: Orientation;
  dualScreenEnabled: boolean;
  /** Bottom screen scale; null means it follows the top screen. */
  bottomScale?: number | null;
  /** Bottom screen rotation; null means it follows the top screen. */
  bottomOrientation?: Orientation | null;
}

export interface HardwareEvent {
  timestamp: string;
  category: EventCategory;
  severity: EventSeverity;
  message: string;
  source: string;
}

// Diagnostics
export interface EvdevDevice {
  eventPath: string;
  name: string;
  phys?: string | null;
  bustype?: string | null;
  vendor?: string | null;
  product?: string | null;
  capEv?: string | null;
  capKey?: string | null;
  capAbs?: string | null;
  capMsc?: string | null;
}

export interface EvdevEvent {
  tsSec: number;
  tsUsec: number;
  typeCode: number;
  code: number;
  value: number;
}

export interface EvdevEventMulti {
  eventPath: string;
  tsSec: number;
  tsUsec: number;
  typeCode: number;
  code: number;
  value: number;
}

export interface HidDevice {
  id: string;
  driver?: string | null;
  hidId?: string | null;
  hidName?: string | null;
  hidPhys?: string | null;
  hidrawNodes: string[];
  inputEventNodes: string[];
}

export interface ReportDescriptor {
  len: number;
  hex: string;
  reportIds: number[];
}

export interface HidrawSample {
  tsMs: number;
  hex: string;
}

export interface HidrawCapture {
  hidrawPath: string;
  samples: HidrawSample[];
  stderr?: string | null;
}
