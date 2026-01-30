import { useMemo, useState } from "react";
import { toast } from "sonner";
import { useTheme } from "next-themes";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useDispatch, useStore, refreshSettings } from "@/lib/store";
import { saveSettings, usbMediaRemapStart, usbMediaRemapStop } from "@/lib/tauri";
import type { DuoSettings, ThemePreference } from "@/types/duo";

export default function Setup() {
  const store = useStore();
  const dispatch = useDispatch();
  const { setTheme } = useTheme();

  const initial = useMemo<DuoSettings>(() => {
    // Ensure sensible defaults even if older backends return settings without new keys.
    return {
      ...store.settings,
      usbMediaRemapEnabled: store.settings.usbMediaRemapEnabled ?? true,
      setupCompleted: store.settings.setupCompleted ?? false,
    };
  }, [store.settings]);

  const [local, setLocal] = useState<DuoSettings>(initial);
  const [saving, setSaving] = useState(false);

  const handleFinish = async () => {
    setSaving(true);
    try {
      const next: DuoSettings = { ...local, setupCompleted: true };
      await saveSettings(next);

      // Apply theme immediately (matches behavior in Settings page).
      const themeMap: Record<ThemePreference, string> = {
        system: "system",
        dark: "dark",
        light: "light",
      };
      setTheme(themeMap[next.theme]);

      // Best-effort: start/stop now so the user sees the effect right away.
      try {
        if (next.usbMediaRemapEnabled) {
          await usbMediaRemapStart();
        } else {
          await usbMediaRemapStop();
        }
      } catch (err) {
        const msg =
          typeof err === "string"
            ? err
            : err && typeof err === "object" && "message" in err
              ? String((err as { message?: unknown }).message)
              : "Failed to apply USB media remap setting";
        toast.error(msg);
      }

      await refreshSettings(dispatch);
      toast.success("Setup complete");
    } catch (err) {
      console.error("Failed to complete setup:", err);
      const msg =
        typeof err === "string"
          ? err
          : err && typeof err === "object" && "message" in err
            ? String((err as { message?: unknown }).message)
            : "Failed to complete setup";
      toast.error(msg);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-screen overflow-hidden bg-background">
      <div className="flex flex-1 items-center justify-center px-6">
        <div className="glass-card w-full max-w-xl rounded-2xl p-6">
          <div className="mb-6">
            <h1 className="text-xl font-semibold tracking-tight">Setup</h1>
            <p className="mt-1 text-sm text-muted-foreground">
              One-time configuration. You can change these later in Settings.
            </p>
          </div>

          <div className="space-y-5">
            <div className="flex items-center justify-between gap-4">
              <div>
                <Label className="text-[13px] font-medium">USB Media Remap</Label>
                <p className="mt-0.5 text-[12px] text-muted-foreground">
                  Maps F1-F3/F5-F6 to media and brightness keys while docked.
                </p>
              </div>
              <div className="flex items-center gap-3">
                <Switch
                  checked={local.usbMediaRemapEnabled}
                  onCheckedChange={(v) =>
                    setLocal((prev) => ({ ...prev, usbMediaRemapEnabled: v }))
                  }
                  disabled={saving}
                />
                <span className="text-[12px] text-muted-foreground">
                  {local.usbMediaRemapEnabled ? "On" : "Off"}
                </span>
              </div>
            </div>

            <p className="text-[12px] text-muted-foreground">
              Enabling this requires admin approval.
            </p>
          </div>

          <div className="mt-6 flex justify-end">
            <Button onClick={handleFinish} disabled={saving}>
              {saving ? "Saving..." : "Finish Setup"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

