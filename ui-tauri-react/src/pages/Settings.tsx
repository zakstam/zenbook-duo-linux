import { useState } from "react";
import { useStore, useDispatch, refreshSettings } from "@/lib/store";
import { saveSettings } from "@/lib/tauri";
import type { DuoSettings, ThemePreference } from "@/types/duo";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { IconDeviceFloppy } from "@tabler/icons-react";

export default function Settings() {
  const store = useStore();
  const dispatch = useDispatch();
  const { setTheme } = useTheme();
  const [saving, setSaving] = useState(false);
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

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Configure default behavior and preferences
        </p>
      </div>

      <div className="glass-card animate-stagger-in stagger-1 rounded-xl p-5">
        <h3 className="mb-5 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          Defaults
        </h3>

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

      <div className="mt-5 flex justify-end animate-stagger-in stagger-2">
        <Button onClick={handleSave} disabled={saving} className="gap-2">
          <IconDeviceFloppy className="size-4" stroke={1.5} />
          {saving ? "Saving..." : "Save Settings"}
        </Button>
      </div>
    </div>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <Label className="text-[13px] font-medium">{label}</Label>
        {description && (
          <p className="mt-0.5 text-[12px] text-muted-foreground">{description}</p>
        )}
      </div>
      {children}
    </div>
  );
}
