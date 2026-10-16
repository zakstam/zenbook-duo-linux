import type { DuoSettings, DuoStatus } from "@/types/duo";
import { normalizeThemePreference } from "@/lib/theme";

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
  fnLock: false,
  keyboardBattery: null,
  micMuted: null,
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
  invertSensorRotation: false,
  setupCompleted: false,
  touchscreenDisabled: [],
  savedDisplayLayout: null,
  keyboardBacklightIdleSecs: 0,
  keyOverrides: {},
};

export function withDuoSettingsDefaults(settings: Partial<DuoSettings>): DuoSettings {
  return {
    ...DEFAULT_DUO_SETTINGS,
    ...settings,
    theme: normalizeThemePreference(settings.theme),
    invertSensorRotation: settings.invertSensorRotation ?? DEFAULT_DUO_SETTINGS.invertSensorRotation,
    touchscreenDisabled: settings.touchscreenDisabled ?? DEFAULT_DUO_SETTINGS.touchscreenDisabled,
    savedDisplayLayout: settings.savedDisplayLayout ?? DEFAULT_DUO_SETTINGS.savedDisplayLayout,
    keyboardBacklightIdleSecs:
      settings.keyboardBacklightIdleSecs ?? DEFAULT_DUO_SETTINGS.keyboardBacklightIdleSecs,
    keyOverrides: settings.keyOverrides ?? DEFAULT_DUO_SETTINGS.keyOverrides,
  };
}
