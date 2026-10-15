import { displayApi } from "@/lib/tauri-adapters";
import { normalizeDuoDisplays } from "@/lib/display-layout";
import type { DisplayInfo, DisplayLayout } from "@/types/duo";

export const DYNAMIC_REFRESH_VALUE = "dynamic";

export async function loadDisplayLayout() {
  return displayApi.getDisplayLayout();
}

/** Applies and saves the layout, then returns where the compositor actually put the screens. */
export async function applyAndPersistDisplayLayout(layout: DisplayLayout) {
  const stacked = normalizeDuoDisplays(layout);
  await displayApi.applyDisplayLayout(stacked);
  await displayApi.saveDisplayLayoutPreference(stacked);
  return displayApi.getDisplayLayout();
}

export function updateDisplayScale(
  layout: DisplayLayout,
  connector: string,
  scale: number,
): DisplayLayout {
  // A new scale changes the top screen's height, so re-stack the bottom one.
  return normalizeDuoDisplays({
    displays: layout.displays.map((display) =>
      display.connector === connector ? { ...display, scale } : display,
    ),
  });
}

export function updateDisplayRefreshMode(
  layout: DisplayLayout,
  connector: string,
  value: string,
): DisplayLayout {
  return {
    displays: layout.displays.map((display) =>
      display.connector === connector
        ? applyRefreshMode(display, value)
        : display,
    ),
  };
}

function applyRefreshMode(display: DisplayInfo, value: string): DisplayInfo {
  if (value === DYNAMIC_REFRESH_VALUE) {
    return { ...display, refreshPolicy: "dynamic" };
  }

  const mode = display.availableModes.find((candidate) => candidate.modeId === value);
  if (!mode) return display;

  return {
    ...display,
    width: mode.width,
    height: mode.height,
    refreshRate: mode.refreshRate,
    currentMode: mode,
    refreshPolicy: "fixed",
  };
}
