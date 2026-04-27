import { useState, useEffect } from "react";
import DisplayCanvas from "@/components/DisplayCanvas";
import { getDisplayLayout, applyDisplayLayout, listTouchscreens, setTouchscreenEnabled, loadSettings, saveSettings } from "@/lib/tauri";
import type { DisplayInfo, DisplayLayout as LayoutType, TouchscreenDevice } from "@/types/duo";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  IconRefresh,
  IconCheck,
  IconAlertTriangle,
} from "@tabler/icons-react";

const DYNAMIC_REFRESH_VALUE = "__dynamic__";

function formatRefreshRate(refreshRate: number) {
  return `${refreshRate.toFixed(1)}Hz`;
}

function refreshSelectValue(display: DisplayInfo) {
  return display.refreshPolicy === "dynamic"
    ? DYNAMIC_REFRESH_VALUE
    : display.currentMode.modeId;
}

function modesForResolution(display: DisplayInfo, width: number, height: number) {
  return display.availableModes.filter(
    (mode) => mode.width === width && mode.height === height
  );
}

export default function DisplayLayout() {
  const [layout, setLayout] = useState<LayoutType>({ displays: [] });
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState("");
  const [touchscreens, setTouchscreens] = useState<TouchscreenDevice[]>([]);

  useEffect(() => {
    listTouchscreens().then(setTouchscreens).catch(console.error);
  }, []);

  const handleTouchToggle = async (connector: string, enabled: boolean) => {
    try {
      await setTouchscreenEnabled(connector, enabled);
      setTouchscreens((prev) =>
        prev.map((ts) => (ts.connector === connector ? { ...ts, enabled } : ts))
      );
      const settings = await loadSettings();
      const disabled = settings.touchscreenDisabled ?? [];
      settings.touchscreenDisabled = enabled
        ? disabled.filter((c) => c !== connector)
        : [...disabled.filter((c) => c !== connector), connector];
      await saveSettings(settings);
    } catch (e) {
      console.error("Failed to toggle touchscreen:", e);
    }
  };

  useEffect(() => {
    getDisplayLayout()
      .then(setLayout)
      .catch((err) => setError(`Failed to get display layout: ${err}`));
  }, []);

  const handleApply = async () => {
    setApplying(true);
    setError("");
    try {
      await applyDisplayLayout(layout);

      const settings = await loadSettings();
      settings.savedDisplayLayout = layout;
      await saveSettings(settings);
    } catch (err) {
      setError(`Failed to apply or save layout: ${err}`);
    } finally {
      setApplying(false);
    }
  };

  const handleRefresh = async () => {
    try {
      const l = await getDisplayLayout();
      setLayout(l);
      setError("");
    } catch (err) {
      setError(`Failed to refresh layout: ${err}`);
    }
  };

  const handleRefreshModeChange = (connector: string, value: string) => {
    setLayout((current) => ({
      displays: current.displays.map((display) => {
        if (display.connector !== connector) return display;

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
      }),
    }));
  };

  return (
    <div>
      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Display Layout</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Arrange and configure connected displays
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={handleRefresh} className="gap-1.5">
            <IconRefresh className="size-3.5" stroke={1.5} />
            Refresh
          </Button>
          <Button size="sm" onClick={handleApply} disabled={applying} className="gap-1.5">
            <IconCheck className="size-3.5" stroke={1.5} />
            {applying ? "Applying..." : "Apply Layout"}
          </Button>
        </div>
      </div>

      {error && (
        <div className="mb-4 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-4 py-3 animate-page-enter">
          <IconAlertTriangle className="size-4 shrink-0 text-destructive" stroke={1.5} />
          <span className="text-[13px] text-destructive">{error}</span>
        </div>
      )}

      <div className="animate-stagger-in stagger-1">
        <DisplayCanvas layout={layout} onLayoutChange={setLayout} />
      </div>

      <div className="glass-card mt-5 rounded-xl p-5 animate-stagger-in stagger-2">
        <h3 className="mb-4 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          Connected Displays
        </h3>
        <div className="space-y-2">
          {layout.displays.map((d) => (
            <div
              key={d.connector}
              className="flex items-center justify-between rounded-lg bg-muted/40 px-3 py-2.5"
            >
              <div className="flex items-center gap-3">
                <span className="font-mono text-[13px] font-medium">{d.connector}</span>
                {d.primary && (
                  <span className="rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary">
                    Primary
                  </span>
                )}
              </div>
              <div className="flex items-center gap-4">
                <span className="font-mono text-[12px] text-muted-foreground">
                  {d.width}x{d.height} @ {formatRefreshRate(d.refreshRate)} | {d.scale}x | ({d.x}, {d.y})
                </span>
                {d.refreshPolicy === "dynamic" && (
                  <span className="rounded bg-emerald-500/12 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-emerald-600 dark:text-emerald-400">
                    Dynamic
                  </span>
                )}
                {(() => {
                  const ts = touchscreens.find((t) => t.connector === d.connector);
                  if (!ts) return null;
                  return (
                    <div className="flex items-center gap-2">
                      <span className="text-[11px] text-muted-foreground">Touch</span>
                      <Switch
                        checked={ts.enabled}
                        onCheckedChange={(checked) =>
                          handleTouchToggle(d.connector, checked)
                        }
                      />
                    </div>
                  );
                })()}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="glass-card mt-4 rounded-xl p-5 animate-stagger-in stagger-3">
        <h3 className="mb-4 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          Per-Display Settings
        </h3>
        <div className="space-y-4">
          {layout.displays.map((d, i) => (
            <div key={d.connector} className="flex items-center justify-between gap-4">
              <Label className="font-mono text-[13px]">{d.connector}</Label>
              <div className="flex items-center gap-3">
                <Select
                  value={String(d.scale)}
                  onValueChange={(v) => {
                    const displays = [...layout.displays];
                    displays[i] = { ...displays[i], scale: parseFloat(v) };
                    setLayout({ displays });
                  }}
                >
                  <SelectTrigger className="w-40">
                    <SelectValue placeholder="Scale" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="1">1.0x</SelectItem>
                    <SelectItem value="1.25">1.25x</SelectItem>
                    <SelectItem value="1.5">1.5x</SelectItem>
                    <SelectItem value="1.75">1.75x</SelectItem>
                    <SelectItem value="1.66">1.66x</SelectItem>
                    <SelectItem value="2">2.0x</SelectItem>
                  </SelectContent>
                </Select>

                <Select
                  value={refreshSelectValue(d)}
                  onValueChange={(value) => handleRefreshModeChange(d.connector, value)}
                >
                  <SelectTrigger className="w-44">
                    <SelectValue placeholder="Refresh rate" />
                  </SelectTrigger>
                  <SelectContent>
                    {d.supportsDynamicRefresh && (
                      <SelectItem value={DYNAMIC_REFRESH_VALUE}>
                        Dynamic
                      </SelectItem>
                    )}
                    {modesForResolution(d, d.currentMode.width, d.currentMode.height).map((mode) => (
                      <SelectItem key={mode.modeId} value={mode.modeId}>
                        {formatRefreshRate(mode.refreshRate)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
