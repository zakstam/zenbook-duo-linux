import { useState } from "react";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { refreshSettings, useDispatch, useStore } from "@/lib/store";
import { saveSettings } from "@/lib/tauri";
import { normalizeThemePreference, resolveThemeForPreference } from "@/lib/theme";
import type { ThemePreference } from "@/types/duo";
import { IconDeviceDesktop, IconMoon, IconSun } from "@tabler/icons-react";

const themeOptions: Array<{
  value: ThemePreference;
  label: string;
  icon: React.ComponentType<{ className?: string; stroke?: number }>;
}> = [
  { value: "system", label: "System", icon: IconDeviceDesktop },
  { value: "light", label: "Light", icon: IconSun },
  { value: "dark", label: "Dark", icon: IconMoon },
];

export default function ThemeToggle() {
  const store = useStore();
  const dispatch = useDispatch();
  const { setTheme } = useTheme();
  const [savingTheme, setSavingTheme] = useState<ThemePreference | null>(null);
  const currentTheme = normalizeThemePreference(store.settings.theme);

  const handleSelect = async (theme: ThemePreference) => {
    if (savingTheme || store.loading || theme === currentTheme) return;

    const previousTheme = currentTheme;
    setSavingTheme(theme);

    try {
      setTheme(await resolveThemeForPreference(theme));
      await saveSettings({ ...store.settings, theme });
      await refreshSettings(dispatch);
    } catch (err) {
      console.error("Failed to save theme preference:", err);
      setTheme(await resolveThemeForPreference(previousTheme));
      toast.error("Failed to save theme preference");
    } finally {
      setSavingTheme(null);
    }
  };

  return (
    <div className="grid grid-cols-3 gap-1 rounded-lg bg-muted/40 p-1">
      {themeOptions.map((option) => {
        const Icon = option.icon;
        const selected = option.value === currentTheme;
        const saving = savingTheme === option.value;

        return (
          <Button
            key={option.value}
            variant={selected ? "secondary" : "ghost"}
            size="sm"
            className="h-8 flex-col gap-0.5 px-1 text-[10px] font-medium"
            onClick={() => void handleSelect(option.value)}
            disabled={savingTheme !== null || store.loading}
            aria-pressed={selected}
          >
            {saving ? (
              <Spinner className="size-3.5" />
            ) : (
              <Icon className="size-3.5" stroke={1.5} />
            )}
            {option.label}
          </Button>
        );
      })}
    </div>
  );
}
