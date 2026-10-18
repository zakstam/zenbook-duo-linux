import { useCallback, useEffect, useRef, useState } from "react";
import { controlsApi } from "@/lib/tauri-adapters";
import { refreshSettings, useDispatch, useStoreSelector } from "@/lib/store";
import { autosavePersistedSetting } from "@/lib/settings-service";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { IconAlertTriangle, IconSun } from "@tabler/icons-react";

type Panel = "top" | "bottom";

/**
 * The Duo panels' brightness, each on its own or both together. GNOME's own
 * slider changes the top panel; with "same brightness" on, the bottom one
 * follows it (the brightness sync).
 */
export default function DisplayBrightnessCard() {
  const dispatch = useDispatch();
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState<Record<Panel, number | null>>({ top: null, bottom: null });
  const inFlight = useRef(false);
  const queued = useRef<{ panel: Panel; percent: number } | null>(null);

  const [levels, setLevels] = useState<Partial<Record<Panel, number>>>({});

  const same = useStoreSelector((s) => s.settings.syncBrightness);
  const keyboardAttached = useStoreSelector((s) => s.status.keyboardAttached);
  const top = dragging.top ?? levels.top ?? 100;
  const bottom = dragging.bottom ?? levels.bottom ?? top;
  const hasBottom = levels.bottom !== undefined;

  const reload = useCallback(async () => {
    try {
      const panels = await controlsApi.getPanelBrightness();
      setLevels(Object.fromEntries(panels.map((p) => [p.panel, p.percent])));
    } catch (err) {
      console.error("Failed to read the brightness:", err);
    }
  }, []);

  // Follow changes made elsewhere too: GNOME's slider, the brightness keys,
  // and "Both Duo screens" in the display controls (F7).
  useEffect(() => {
    void reload();
    const timer = setInterval(() => {
      void reload();
      void refreshSettings(dispatch);
    }, 2000);
    return () => clearInterval(timer);
  }, [reload, dispatch]);

  // One request at a time while dragging; the last value always gets sent.
  const send = async (panel: Panel, percent: number) => {
    if (inFlight.current) {
      queued.current = { panel, percent };
      return;
    }
    inFlight.current = true;
    try {
      await controlsApi.setPanelBrightness(same ? "both" : panel, percent);
      setError(null);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to set the brightness");
    } finally {
      inFlight.current = false;
      const next = queued.current;
      queued.current = null;
      if (next) void send(next.panel, next.percent);
    }
  };

  const change = (panel: Panel, value: number[]) => {
    const percent = value[0];
    if (percent === undefined) return;
    setDragging(same ? { top: percent, bottom: percent } : { ...dragging, [panel]: percent });
    void send(panel, percent);
  };

  const commit = async () => {
    await reload();
    setDragging({ top: null, bottom: null });
  };

  const setSame = async (on: boolean) => {
    try {
      await autosavePersistedSetting("syncBrightness", on, dispatch);
      if (on) await send("top", top);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to save the setting");
    }
  };

  const row = (panel: Panel, label: string, value: number, disabled = false) => (
    <div className="flex items-center gap-4">
      <span className="w-24 shrink-0 text-[12px]">{label}</span>
      <Slider
        min={1}
        max={100}
        step={1}
        value={[value]}
        onValueChange={(v) => change(panel, v)}
        onValueCommit={() => void commit()}
        disabled={disabled}
        className="flex-1"
      />
      <span className="w-10 shrink-0 text-right font-mono text-[11px] text-muted-foreground">
        {value}%
      </span>
    </div>
  );

  return (
    <div className="glass-card rounded-xl p-5">
      <div className="mb-5 flex items-center gap-2.5">
        <div className="flex size-7 items-center justify-center rounded-lg bg-yellow-500/12 text-yellow-500 dark:bg-yellow-400/10 dark:text-yellow-400">
          <IconSun className="size-3.5" stroke={1.75} />
        </div>
        <div>
          <h3 className="text-[13px] font-semibold text-foreground">Display Brightness</h3>
          <p className="text-[11px] text-muted-foreground">The two Duo screens</p>
        </div>
      </div>

      <div className="space-y-3">
        {row("top", "Top screen", top)}
        {hasBottom &&
          row("bottom", "Bottom screen", same ? top : bottom, keyboardAttached)}
        {hasBottom && keyboardAttached && (
          <p className="text-[11px] text-muted-foreground">
            The bottom screen is off while the keyboard is attached.
          </p>
        )}
      </div>

      {hasBottom && (
        <div className="mt-5 flex items-center justify-between gap-4 border-t border-border/50 pt-4">
          <div>
            <p className="text-[13px]">Same brightness for both Duo screens</p>
            <p className="text-[11px] text-muted-foreground">
              {same
                ? "The bottom screen follows the top one, however it is changed"
                : "Each screen keeps its own brightness"}
            </p>
          </div>
          <Switch checked={same} onCheckedChange={(on) => void setSame(on)} />
        </div>
      )}

      {error && (
        <div className="mt-4 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[12px] text-destructive">
          <IconAlertTriangle className="size-3.5 shrink-0" stroke={1.5} />
          <span>{error}</span>
        </div>
      )}
    </div>
  );
}
