export type ConnectionType = "usb" | "bluetooth" | "none";
export type Orientation = "normal" | "left" | "right" | "inverted";
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
}

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
}

export interface DisplayLayout {
  displays: DisplayInfo[];
}

export interface DuoSettings {
  defaultBacklight: number;
  defaultScale: number;
  autoDualScreen: boolean;
  syncBrightness: boolean;
  theme: ThemePreference;
  usbMediaRemapEnabled: boolean;
  setupCompleted: boolean;
}

export interface UsbMediaRemapStatus {
  running: boolean;
  pid?: number | null;
}

export interface Profile {
  id: string;
  name: string;
  backlightLevel: number;
  scale: number;
  orientation: Orientation;
  dualScreenEnabled: boolean;
  displayLayout: DisplayLayout | null;
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
