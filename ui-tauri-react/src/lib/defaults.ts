import type { DuoSettings, DuoStatus } from "@/types/duo";

export const DEFAULT_DUO_STATUS: DuoStatus = {
  keyboardAttached: false,
  connectionType: "none",
  monitorCount: 0,
  wifiEnabled: false,
  bluetoothEnabled: false,
  backlightLevel: 0,
  displayBrightness: 0,
  maxBrightness: 1,
  serviceActive: false,
  orientation: "normal",
};

export const DEFAULT_DUO_SETTINGS: DuoSettings = {
  defaultBacklight: 0,
  defaultScale: 1.66,
  autoDualScreen: true,
  syncBrightness: true,
  theme: "system",
  usbMediaRemapEnabled: true,
  startOnBootMinimized: false,
  setupCompleted: false,
  touchscreenDisabled: [],
  savedDisplayLayout: null,
};

export function withDuoSettingsDefaults(settings: Partial<DuoSettings>): DuoSettings {
  return {
    ...DEFAULT_DUO_SETTINGS,
    ...settings,
    touchscreenDisabled: settings.touchscreenDisabled ?? DEFAULT_DUO_SETTINGS.touchscreenDisabled,
    savedDisplayLayout: settings.savedDisplayLayout ?? DEFAULT_DUO_SETTINGS.savedDisplayLayout,
  };
}
