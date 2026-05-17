import { withDuoSettingsDefaults } from "@/lib/defaults";
import { eventsApi, logsApi, profilesApi, settingsApi, statusApi } from "@/lib/tauri-adapters";
import type { StoreAction } from "@/lib/store";
import type { Dispatch } from "react";

const RECENT_EVENT_COUNT = 100;
const LOG_LINE_COUNT = 500;

function logRefreshFailure(label: string, error: unknown) {
  console.error(label, error);
}

export async function refreshStatus(dispatch: Dispatch<StoreAction>) {
  try {
    const status = await statusApi.getStatus();
    dispatch({ type: "SET_STATUS", payload: status });
  } catch (e) {
    logRefreshFailure("Failed to fetch status:", e);
  }
}

export async function refreshSettings(dispatch: Dispatch<StoreAction>) {
  try {
    const settings = await settingsApi.loadSettings();
    dispatch({ type: "SET_SETTINGS", payload: withDuoSettingsDefaults(settings) });
  } catch (e) {
    logRefreshFailure("Failed to load settings:", e);
  }
}

export async function refreshProfiles(dispatch: Dispatch<StoreAction>) {
  try {
    const profiles = await profilesApi.listProfiles();
    dispatch({ type: "SET_PROFILES", payload: profiles });
  } catch (e) {
    logRefreshFailure("Failed to load profiles:", e);
  }
}

export async function refreshLogs(dispatch: Dispatch<StoreAction>) {
  try {
    const logs = await logsApi.readLog(LOG_LINE_COUNT);
    dispatch({ type: "SET_LOGS", payload: logs });
  } catch (e) {
    logRefreshFailure("Failed to read logs:", e);
  }
}

export async function clearLogs(dispatch: Dispatch<StoreAction>) {
  try {
    await logsApi.clearLog();
    dispatch({ type: "SET_LOGS", payload: [] });
  } catch (e) {
    logRefreshFailure("Failed to clear logs:", e);
  }
}

export async function refreshEvents(dispatch: Dispatch<StoreAction>) {
  try {
    const events = await eventsApi.getRecentEvents(RECENT_EVENT_COUNT);
    dispatch({ type: "SET_EVENTS", payload: events });
  } catch (e) {
    logRefreshFailure("Failed to get events:", e);
  }
}

export async function refreshVersionInfo(dispatch: Dispatch<StoreAction>) {
  try {
    const versionInfo = await statusApi.getVersionInfo();
    dispatch({ type: "SET_VERSION_INFO", payload: versionInfo });
  } catch (e) {
    logRefreshFailure("Failed to get version info:", e);
  }
}

export async function loadInitialStoreState(dispatch: Dispatch<StoreAction>) {
  await Promise.all([
    refreshStatus(dispatch),
    refreshSettings(dispatch),
    refreshProfiles(dispatch),
    refreshLogs(dispatch),
    refreshEvents(dispatch),
    refreshVersionInfo(dispatch),
  ]);
}

export function subscribeStoreEvents(dispatch: Dispatch<StoreAction>) {
  return [
    statusApi.onStatusChanged(() => {
      refreshStatus(dispatch);
      refreshVersionInfo(dispatch);
    }),
    logsApi.onLogUpdated(() => refreshLogs(dispatch)),
    eventsApi.onHardwareEvent(() => refreshEvents(dispatch)),
  ];
}
