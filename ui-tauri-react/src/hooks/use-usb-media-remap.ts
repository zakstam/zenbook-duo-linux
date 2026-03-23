import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { refreshSettings, useDispatch, useStore } from "@/lib/store";
import {
  saveSettings,
  usbMediaRemapStart,
  usbMediaRemapStatus,
  usbMediaRemapStop,
  usbMediaRemapTogglePause,
} from "@/lib/tauri";
import type { DuoSettings, UsbMediaRemapStatus } from "@/types/duo";

const defaultRemapStatus: UsbMediaRemapStatus = {
  running: false,
  pid: null,
  paused: false,
};

interface UseUsbMediaRemapOptions {
  settings?: DuoSettings;
  onSettingsSaved?: (settings: DuoSettings) => void;
}

export function useUsbMediaRemap(options: UseUsbMediaRemapOptions = {}) {
  const store = useStore();
  const dispatch = useDispatch();
  const isUsb = store.status.connectionType === "usb";
  const [remapBusy, setRemapBusy] = useState(false);
  const [remapStatus, setRemapStatus] =
    useState<UsbMediaRemapStatus>(defaultRemapStatus);
  const [remapDesired, setRemapDesired] = useState<boolean | null>(null);
  const remapOpIdRef = useRef(0);
  const remapDesiredRef = useRef<boolean | null>(null);
  const settings = options.settings ?? store.settings;

  useEffect(() => {
    remapDesiredRef.current = remapDesired;
  }, [remapDesired]);

  const refreshRemapStatus = async () => {
    try {
      const status = await usbMediaRemapStatus();
      setRemapStatus(status);
      setRemapDesired((desired) =>
        desired !== null && status.running === desired ? null : desired
      );
    } catch (err) {
      console.error("Failed to read USB remap status:", err);
    }
  };

  const persistRemapPreference = async (nextEnabled: boolean) => {
    const nextSettings = {
      ...settings,
      usbMediaRemapEnabled: nextEnabled,
    };

    options.onSettingsSaved?.(nextSettings);
    await saveSettings(nextSettings);
    await refreshSettings(dispatch);
  };

  const setEnabled = async (nextEnabled: boolean) => {
    if (!isUsb) {
      toast.error(
        "USB Media Remap is only available when the keyboard is connected via USB."
      );
      return;
    }

    setRemapBusy(true);
    const opId = (remapOpIdRef.current += 1);
    setRemapDesired(nextEnabled);

    try {
      if (nextEnabled) {
        await usbMediaRemapStart();
      } else {
        await usbMediaRemapStop();
      }

      await persistRemapPreference(nextEnabled);
      toast.success(
        nextEnabled ? "USB media remap enabled" : "USB media remap disabled"
      );
    } catch (err) {
      console.error("Failed to toggle USB remap:", err);
      if (remapOpIdRef.current === opId) {
        setRemapDesired(null);
      }

      const msg =
        typeof err === "string"
          ? err
          : err && typeof err === "object" && "message" in err
            ? String((err as { message?: unknown }).message)
            : "Failed to toggle USB media remap";
      toast.error(msg);
    } finally {
      setRemapBusy(false);
      void refreshRemapStatus();

      let attempts = 0;
      const interval = setInterval(() => {
        attempts += 1;
        void refreshRemapStatus();
        if (remapDesiredRef.current === null || attempts >= 80) {
          clearInterval(interval);
        }
      }, 250);
    }
  };

  const togglePause = async () => {
    try {
      await usbMediaRemapTogglePause();
      await refreshRemapStatus();
    } catch (err) {
      console.error("Failed to toggle pause:", err);
      toast.error("Failed to toggle pause");
    }
  };

  useEffect(() => {
    void refreshRemapStatus();

    const interval = setInterval(() => {
      void refreshRemapStatus();
    }, 1500);

    return () => clearInterval(interval);
  }, [store.status.connectionType]);

  const statusLabel =
    remapDesired !== null
      ? remapDesired
        ? "Enabling..."
        : "Disabling..."
      : remapStatus.running
        ? remapStatus.paused
          ? "Paused"
          : "On"
        : "Off";

  return {
    isUsb,
    remapBusy,
    remapDesired,
    remapStatus,
    statusLabel,
    controlsDisabled: !isUsb || remapBusy || remapDesired !== null,
    refreshRemapStatus,
    setEnabled,
    togglePause,
  };
}
