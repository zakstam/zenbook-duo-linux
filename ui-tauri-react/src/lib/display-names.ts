import { createContext, useEffect, useState } from "react";
import { displayApi } from "@/lib/tauri-adapters";
import type { DisplayLayout } from "@/types/duo";

/** The monitors' names by connector, as GNOME Settings shows them. */
export const MonitorNamesContext = createContext<Record<string, string>>({});

/**
 * What a screen is called, the same as in the display controls (F7): the
 * Duo panels by position, other screens by their name.
 */
export function displayLabel(connector: string, names: Record<string, string>) {
  if (connector === "eDP-1") return "Top screen";
  if (connector === "eDP-2") return "Bottom screen";
  return names[connector] ?? connector;
}

/** The monitors' names, read again when the connected screens change. */
export function useMonitorNames(layout: DisplayLayout) {
  const [names, setNames] = useState<Record<string, string>>({});
  const connectors = layout.displays.map((d) => d.connector).sort().join(",");
  useEffect(() => {
    let cancelled = false;
    displayApi
      .getMonitorNames()
      .then((result) => {
        if (!cancelled) setNames(result);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [connectors]);
  return names;
}
