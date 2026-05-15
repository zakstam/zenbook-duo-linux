import { getCurrentWindow, type Theme } from "@tauri-apps/api/window";
import { getSystemTheme } from "@/lib/tauri";
import type { ThemePreference } from "@/types/duo";

export const THEME_PREFERENCES = ["system", "light", "dark"] as const satisfies readonly ThemePreference[];
export type ExplicitThemePreference = Extract<ThemePreference, "light" | "dark">;

export function isThemePreference(value: unknown): value is ThemePreference {
  return typeof value === "string" && THEME_PREFERENCES.includes(value as ThemePreference);
}

export function normalizeThemePreference(value: unknown): ThemePreference {
  return isThemePreference(value) ? value : "system";
}

export function toNextTheme(value: unknown): ThemePreference {
  return normalizeThemePreference(value);
}

export async function resolveThemeForPreference(value: unknown): Promise<ThemePreference> {
  const preference = normalizeThemePreference(value);
  if (preference !== "system") return preference;

  return (await getBackendSystemTheme()) ?? (await getNativeSystemTheme()) ?? "system";
}

export function nextExplicitTheme(resolvedTheme: string | undefined): ExplicitThemePreference {
  return resolvedTheme === "dark" ? "light" : "dark";
}

export function isExplicitThemePreference(value: unknown): value is ExplicitThemePreference {
  return value === "light" || value === "dark";
}

export async function getBackendSystemTheme(): Promise<ExplicitThemePreference | null> {
  try {
    const theme = await getSystemTheme();
    return isExplicitThemePreference(theme) ? theme : null;
  } catch (err) {
    console.warn("Failed to read backend system theme:", err);
    return null;
  }
}

export async function getNativeSystemTheme(): Promise<ExplicitThemePreference | null> {
  try {
    const theme = await getCurrentWindow().theme();
    return isExplicitThemePreference(theme) ? theme : null;
  } catch (err) {
    console.warn("Failed to read native system theme:", err);
    return null;
  }
}

export async function onNativeSystemThemeChanged(
  handler: (theme: ExplicitThemePreference) => void,
): Promise<() => void> {
  try {
    return await getCurrentWindow().onThemeChanged(({ payload }: { payload: Theme }) => {
      if (isExplicitThemePreference(payload)) {
        handler(payload);
      }
    });
  } catch (err) {
    console.warn("Failed to listen for native system theme changes:", err);
    return () => {};
  }
}
