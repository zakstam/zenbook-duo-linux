import { controlsApi } from "@/lib/tauri-adapters";
import type { UsbMediaRemapStatus } from "@/types/duo";

export const defaultUsbMediaRemapStatus: UsbMediaRemapStatus = {
  running: false,
  pid: null,
  paused: false,
};

export function usbMediaRemapStatusLabel(
  status: UsbMediaRemapStatus,
  desired: boolean | null,
) {
  if (desired !== null) {
    return desired ? "Enabling..." : "Disabling...";
  }
  if (!status.running) return "Off";
  return status.paused ? "Paused" : "On";
}

export async function readUsbMediaRemapStatus() {
  return controlsApi.usbMediaRemapStatus();
}

export async function setUsbMediaRemapEnabled(enabled: boolean) {
  if (enabled) {
    await controlsApi.usbMediaRemapStart();
  } else {
    await controlsApi.usbMediaRemapStop();
  }
}

export async function toggleUsbMediaRemapPause() {
  await controlsApi.usbMediaRemapTogglePause();
}

export function remapErrorMessage(err: unknown, fallback = "Failed to toggle USB media remap") {
  return typeof err === "string"
    ? err
    : err && typeof err === "object" && "message" in err
      ? String((err as { message?: unknown }).message)
      : fallback;
}
