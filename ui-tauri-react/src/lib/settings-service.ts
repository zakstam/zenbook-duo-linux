import { refreshSettings, type AppState, type StoreAction } from "@/lib/store";
import { settingsApi } from "@/lib/tauri-adapters";
import { withDuoSettingsDefaults } from "@/lib/defaults";
import { resolveThemeForPreference } from "@/lib/theme";
import type { DuoSettings } from "@/types/duo";
import type { Dispatch } from "react";

export function createSettingsDraft(settings: DuoSettings): DuoSettings {
  return { ...withDuoSettingsDefaults(settings) };
}

export function patchSettingsDraft(
  draft: DuoSettings,
  key: keyof DuoSettings,
  value: unknown,
): DuoSettings {
  return { ...draft, [key]: value };
}

export async function saveSettingsDraft(
  draft: DuoSettings,
  dispatch: Dispatch<StoreAction>,
  setTheme: (theme: string) => void,
) {
  await settingsApi.saveSettings(draft);
  await refreshSettings(dispatch);
  setTheme(await resolveThemeForPreference(draft.theme));
}

export async function autosavePersistedSetting<K extends keyof DuoSettings>(
  key: K,
  value: DuoSettings[K],
  dispatch: Dispatch<StoreAction>,
) {
  const persisted = await settingsApi.loadSettings();
  const next = patchSettingsDraft(withDuoSettingsDefaults(persisted), key, value);
  await settingsApi.saveSettings(next);
  await refreshSettings(dispatch);
  return next;
}

export function setupCompletionSettings(state: Pick<AppState, "settings">, patch: Partial<DuoSettings> = {}) {
  return {
    ...withDuoSettingsDefaults(state.settings),
    ...patch,
    setupCompleted: true,
  };
}
