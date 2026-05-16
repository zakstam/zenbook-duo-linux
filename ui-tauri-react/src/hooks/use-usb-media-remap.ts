import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { refreshSettings, useDispatch, useStore } from "@/lib/store";
import { saveSettings } from "@/lib/tauri";
import {
  defaultUsbMediaRemapStatus,
  readUsbMediaRemapStatus,
  remapErrorMessage,
  setUsbMediaRemapEnabled,
  toggleUsbMediaRemapPause,
  usbMediaRemapStatusLabel,
} from "@/lib/usb-media-remap-controller";
import type { DuoSettings } from "@/types/duo";

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
    useState(defaultUsbMediaRemapStatus);
  const [remapDesired, setRemapDesired] = useState<boolean | null>(null);
  const remapOpIdRef = useRef(0);
  const remapDesiredRef = useRef<boolean | null>(null);
  const settings = options.settings ?? store.settings;

  useEffect(() => {
    remapDesiredRef.current = remapDesired;
  }, [remapDesired]);

  const refreshRemapStatus = async () => {
    try {
      const status = await readUsbMediaRemapStatus();
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
      await setUsbMediaRemapEnabled(nextEnabled);

      await persistRemapPreference(nextEnabled);
      toast.success(
        nextEnabled ? "USB media remap enabled" : "USB media remap disabled"
      );
    } catch (err) {
      console.error("Failed to toggle USB remap:", err);
      if (remapOpIdRef.current === opId) {
        setRemapDesired(null);
      }

      toast.error(remapErrorMessage(err));
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
      await toggleUsbMediaRemapPause();
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

  const statusLabel = usbMediaRemapStatusLabel(remapStatus, remapDesired);

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
