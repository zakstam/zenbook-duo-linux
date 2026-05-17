import { useCallback, useEffect, useState } from "react";
import { controlsApi } from "@/lib/tauri-adapters";
import type { TouchscreenDevice } from "@/types/duo";

export function useTouchscreens() {
  const [touchscreens, setTouchscreens] = useState<TouchscreenDevice[]>([]);
  const [pendingConnector, setPendingConnector] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      setTouchscreens(await controlsApi.listTouchscreens());
    } catch (err) {
      console.error("Failed to list touchscreens:", err);
      setError("Failed to list touchscreens");
    }
  }, []);

  const setEnabled = useCallback(async (connector: string, enabled: boolean) => {
    const previous = touchscreens;
    setPendingConnector(connector);
    setError(null);
    setTouchscreens((current) =>
      current.map((touchscreen) =>
        touchscreen.connector === connector ? { ...touchscreen, enabled } : touchscreen,
      ),
    );

    try {
      await controlsApi.setTouchscreenEnabled(connector, enabled);
      await controlsApi.saveTouchscreenPreference(connector, enabled);
    } catch (err) {
      console.error("Failed to toggle touchscreen:", err);
      setTouchscreens(previous);
      setError("Failed to toggle touchscreen");
    } finally {
      setPendingConnector(null);
    }
  }, [touchscreens]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { touchscreens, pendingConnector, error, refresh, setEnabled };
}
