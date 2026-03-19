import { useEffect, useRef, useState } from "react";
import { useStore, useDispatch, refreshSettings } from "@/lib/store";
import {
  saveSettings,
  usbMediaRemapStart,
  usbMediaRemapStatus,
  usbMediaRemapStop,
  usbMediaRemapTogglePause,
} from "@/lib/tauri";
import type { DuoSettings, ThemePreference, UsbMediaRemapStatus } from "@/types/duo";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { IconDeviceFloppy, IconKeyboard, IconPlayerPause, IconPlayerPlay } from "@tabler/icons-react";

export default function Settings() {
  const store = useStore();
  const dispatch = useDispatch();
  const { setTheme } = useTheme();
  const isUsb = store.status.connectionType === "usb";
  const [saving, setSaving] = useState(false);
  const [remapBusy, setRemapBusy] = useState(false);
  const [remapStatus, setRemapStatus] = useState<UsbMediaRemapStatus>({
    running: false,
    pid: null,
    paused: false,
  });
  // Desired switch state while we wait for pkexec + pid-file status to catch up.
  const [remapDesired, setRemapDesired] = useState<boolean | null>(null);
  const remapOpIdRef = useRef(0);
  const remapDesiredRef = useRef<boolean | null>(null);

  useEffect(() => {
    remapDesiredRef.current = remapDesired;
  }, [remapDesired]);
  const [localSettings, setLocalSettings] = useState<DuoSettings>({
    ...store.settings,
  });

  const updateLocal = (key: keyof DuoSettings, value: unknown) => {
    setLocalSettings((prev) => ({ ...prev, [key]: value }));
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await saveSettings(localSettings);
      await refreshSettings(dispatch);

      const themeMap: Record<ThemePreference, string> = {
        system: "system",
        dark: "dark",
        light: "light",
      };
      setTheme(themeMap[localSettings.theme]);

      toast.success("Settings saved");
    } catch (err) {
      console.error("Failed to save settings:", err);
      toast.error("Failed to save settings");
    } finally {
      setSaving(false);
    }
  };

  const refreshRemapStatus = async () => {
    try {
      const status = await usbMediaRemapStatus();
      setRemapStatus(status);
      // Use a functional update to avoid stale-closure issues with timers.
      setRemapDesired((desired) =>
        desired !== null && status.running === desired ? null : desired
      );
    } catch (err) {
      console.error("Failed to read USB remap status:", err);
    }
  };

  const handleRemapToggle = async (nextEnabled: boolean) => {
    if (!isUsb) {
      toast.error("USB Media Remap is only available when the keyboard is connected via USB.");
      return;
    }
    setRemapBusy(true);
    const opId = (remapOpIdRef.current += 1);
    setRemapDesired(nextEnabled);
    try {
      const nextSettings = {
        ...localSettings,
        usbMediaRemapEnabled: nextEnabled,
      };

      if (nextEnabled) {
        await usbMediaRemapStart();
      } else {
        await usbMediaRemapStop();
      }

      setLocalSettings(nextSettings);
      await saveSettings(nextSettings);
      await refreshSettings(dispatch);
      toast.success(nextEnabled ? "USB media remap enabled" : "USB media remap disabled");
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
      // Refresh immediately, then keep polling briefly to allow the stop/start to complete.
      void refreshRemapStatus();
      let attempts = 0;
      const interval = setInterval(() => {
        attempts += 1;
        void refreshRemapStatus();
        // Stop polling once the backend converged, or after a reasonable timeout.
        if (remapDesiredRef.current === null || attempts >= 80) {
          clearInterval(interval);
        }
      }, 250);
    }
  };

  useEffect(() => {
    void refreshRemapStatus();
  }, []);

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Configure default behavior and preferences
        </p>
      </div>

      <div className="glass-card animate-stagger-in stagger-1 rounded-xl p-5">
        <div className="mb-5 flex items-center justify-between gap-4">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <IconDeviceFloppy className="size-3.5" stroke={1.75} />
            </div>
            <h3 className="text-[13px] font-semibold text-foreground">
              Defaults
            </h3>
          </div>
          <Button onClick={handleSave} disabled={saving} size="sm" className="gap-2">
            <IconDeviceFloppy className="size-3.5" stroke={1.5} />
            {saving ? "Saving..." : "Save"}
          </Button>
        </div>

        <div className="space-y-5">
          <SettingRow label="Default Backlight Level" description="Applied when the keyboard connects">
            <Select
              value={String(localSettings.defaultBacklight)}
              onValueChange={(v) => updateLocal("defaultBacklight", parseInt(v))}
            >
              <SelectTrigger className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="0">0 - Off</SelectItem>
                <SelectItem value="1">1 - Low</SelectItem>
                <SelectItem value="2">2 - Medium</SelectItem>
                <SelectItem value="3">3 - High</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <div className="h-px bg-border/50" />

          <SettingRow label="Default Display Scale" description="Scale factor for newly connected displays">
            <Select
              value={String(localSettings.defaultScale)}
              onValueChange={(v) => updateLocal("defaultScale", parseFloat(v))}
            >
              <SelectTrigger className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="1">1.0x (100%)</SelectItem>
                <SelectItem value="1.25">1.25x (125%)</SelectItem>
                <SelectItem value="1.5">1.5x (150%)</SelectItem>
                <SelectItem value="1.66">1.66x (166%)</SelectItem>
                <SelectItem value="2">2.0x (200%)</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>

          <div className="h-px bg-border/50" />

          <SettingRow label="Theme" description="Application appearance">
            <Select
              value={localSettings.theme}
              onValueChange={(v) => updateLocal("theme", v as ThemePreference)}
            >
              <SelectTrigger className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="system">System</SelectItem>
                <SelectItem value="dark">Dark</SelectItem>
                <SelectItem value="light">Light</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>
        </div>
      </div>

      <div className="mt-5 glass-card animate-stagger-in stagger-2 rounded-xl p-5">
        <div className="mb-5 flex items-center gap-2.5">
          <div className="flex size-7 items-center justify-center rounded-lg bg-teal-500/10 text-teal-500 dark:bg-teal-400/10 dark:text-teal-400">
            <IconKeyboard className="size-3.5" stroke={1.75} />
          </div>
          <h3 className="text-[13px] font-semibold text-foreground">
            Keyboard
          </h3>
        </div>

        <div className="space-y-4">
          <SettingRow
            label="USB Media Remap"
            description="Maps F1-F3/F5-F6 to media and brightness keys while docked"
            labelExtra={
              store.status.connectionType === "bluetooth" ? (
                <Badge
                  className="border-amber-500/20 bg-amber-500/10 text-amber-700 dark:border-amber-400/25 dark:bg-amber-400/10 dark:text-amber-200"
                >
                  Unavailable on bluetooth
                </Badge>
              ) : null
            }
          >
            <div className="flex items-center gap-3">
              <Switch
                checked={remapDesired ?? remapStatus.running}
                onCheckedChange={handleRemapToggle}
                disabled={!isUsb || remapBusy || remapDesired !== null}
              />
              <span className="text-[12px] text-muted-foreground">
                {remapDesired !== null
                  ? remapDesired
                    ? "Enabling..."
                    : "Disabling..."
                  : remapStatus.running
                    ? remapStatus.paused
                      ? "Paused"
                      : "On"
                    : "Off"}
              </span>
              {remapStatus.running && remapDesired === null && (
                <Button
                  size="sm"
                  variant={remapStatus.paused ? "default" : "outline"}
                  className="gap-1.5"
                  onClick={async () => {
                    try {
                      await usbMediaRemapTogglePause();
                      await refreshRemapStatus();
                    } catch (err) {
                      console.error("Failed to toggle pause:", err);
                      toast.error("Failed to toggle pause");
                    }
                  }}
                >
                  {remapStatus.paused ? (
                    <><IconPlayerPlay className="size-3.5" stroke={1.5} /> Resume</>
                  ) : (
                    <><IconPlayerPause className="size-3.5" stroke={1.5} /> Pause</>
                  )}
                </Button>
              )}
              {remapStatus.running && remapStatus.paused && (
                <Badge
                  className="border-amber-500/20 bg-amber-500/10 text-amber-700 dark:border-amber-400/25 dark:bg-amber-400/10 dark:text-amber-200"
                >
                  Paused
                </Badge>
              )}
              {(remapBusy || remapDesired !== null) && <Spinner className="text-muted-foreground" />}
            </div>
          </SettingRow>

          <p className="rounded-lg bg-muted/50 px-3 py-2 text-[12px] text-muted-foreground">
            Requires admin approval and restarts input handling while enabled.
            You can pause/resume from the button above or from the system tray.
            For a keyboard shortcut, add a compositor keybinding that runs{" "}
            <code className="rounded bg-muted px-1 py-0.5 text-[11px]">
              zenbook-duo-control --toggle-remap-pause
            </code>.
          </p>
        </div>
      </div>

    </div>
  );
}

function SettingRow({
  label,
  description,
  labelExtra,
  children,
}: {
  label: string;
  description?: string;
  labelExtra?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="flex items-center gap-2">
          <Label className="text-[13px] font-medium">{label}</Label>
          {labelExtra}
        </div>
        {description && (
          <p className="mt-0.5 text-[12px] text-muted-foreground">{description}</p>
        )}
      </div>
      {children}
    </div>
  );
}
