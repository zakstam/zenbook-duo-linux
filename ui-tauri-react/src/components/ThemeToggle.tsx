import { useState } from "react";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { Spinner } from "@/components/ui/spinner";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { refreshSettings, useDispatch, useStore } from "@/lib/store";
import { settingsApi } from "@/lib/tauri-adapters";
import { normalizeThemePreference, resolveThemeForPreference } from "@/lib/theme";
import type { ThemePreference } from "@/types/duo";
import { cn } from "@/lib/utils";
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

const SEGMENT_EASE = "cubic-bezier(0.32, 0.72, 0, 1)";

export default function ThemeToggle() {
  const store = useStore();
  const dispatch = useDispatch();
  const { setTheme } = useTheme();
  const [savingTheme, setSavingTheme] = useState<ThemePreference | null>(null);
  const currentTheme = normalizeThemePreference(store.settings.theme);
  const currentIndex = themeOptions.findIndex((o) => o.value === currentTheme);
  const currentLabel =
    themeOptions.find((o) => o.value === currentTheme)?.label ?? "System";
  const busy = savingTheme !== null || store.loading;

  const handleSelect = async (theme: ThemePreference) => {
    if (savingTheme || store.loading || theme === currentTheme) return;

    const previousTheme = currentTheme;
    setSavingTheme(theme);

    try {
      setTheme(await resolveThemeForPreference(theme));
      await settingsApi.saveSettings({ ...store.settings, theme });
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
    <div className="space-y-2">
      {/* Header: section label + current mode read-out */}
      <div className="flex items-center justify-between px-0.5">
        <span className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
          Theme
        </span>
        <span className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
          <span
            aria-hidden
            className={cn(
              "size-1.5 rounded-full bg-primary shadow-[0_0_6px_currentColor] text-primary",
              currentTheme === "system" && "animate-pulse-glow",
            )}
          />
          {currentLabel}
        </span>
      </div>

      {/* Segmented control */}
      <div
        className={cn(
          "relative grid grid-cols-3 rounded-xl p-1",
          "bg-muted/40 ring-1 ring-inset ring-border/50",
          "shadow-[inset_0_1px_2px_oklch(0_0_0/0.05)]",
        )}
      >
        {/* Halo glow following the pill */}
        <div
          aria-hidden
          className="pointer-events-none absolute inset-y-0 left-1 w-[calc((100%-0.5rem)/3)]"
          style={{
            transform: `translateX(${currentIndex * 100}%)`,
            transition: `transform 420ms ${SEGMENT_EASE}`,
          }}
        >
          <div className="absolute inset-x-2 top-1/2 h-7 -translate-y-1/2 rounded-full bg-primary/30 blur-lg" />
        </div>

        {/* Sliding pill indicator */}
        <div
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-y-1 left-1 w-[calc((100%-0.5rem)/3)] rounded-lg",
            "bg-card ring-1 ring-border/70",
            "shadow-[0_1px_2px_oklch(0_0_0/0.08),inset_0_1px_0_oklch(1_0_0/0.06)]",
          )}
          style={{
            transform: `translateX(${currentIndex * 100}%)`,
            transition: `transform 420ms ${SEGMENT_EASE}`,
          }}
        >
          <div className="absolute inset-0 rounded-lg bg-gradient-to-b from-foreground/[0.04] to-transparent" />
        </div>

        {themeOptions.map((option) => {
          const Icon = option.icon;
          const selected = option.value === currentTheme;
          const saving = savingTheme === option.value;

          return (
            <Tooltip key={option.value} delayDuration={350}>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => void handleSelect(option.value)}
                  disabled={busy}
                  aria-pressed={selected}
                  aria-label={`${option.label} theme`}
                  className={cn(
                    "relative z-10 flex h-8 items-center justify-center rounded-lg",
                    "transition-[color,transform] duration-200",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    "disabled:cursor-not-allowed",
                    selected
                      ? "text-foreground"
                      : "text-muted-foreground/70 hover:text-foreground active:scale-[0.94]",
                  )}
                >
                  {saving ? (
                    <Spinner className="size-4 text-primary" />
                  ) : (
                    <Icon
                      className="size-4 transition-transform duration-300"
                      stroke={selected ? 1.9 : 1.5}
                    />
                  )}
                </button>
              </TooltipTrigger>
              <TooltipContent side="top" sideOffset={6} className="text-[10px] tracking-wide">
                {option.label}
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}
