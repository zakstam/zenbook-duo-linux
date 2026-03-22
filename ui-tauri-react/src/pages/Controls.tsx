import { useState, useEffect } from "react";
import BacklightSlider from "@/components/BacklightSlider";
import OrientationButtons from "@/components/OrientationButtons";
import { restartService, listTouchscreens, setTouchscreenEnabled, loadSettings, saveSettings } from "@/lib/tauri";
import type { TouchscreenDevice } from "@/types/duo";
import { Switch } from "@/components/ui/switch";
import { refreshStatus, useDispatch, useStore } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  IconRefresh,
  IconKeyboard,
  IconRotate,
  IconServer,
  IconCheck,
  IconHandFinger,
} from "@tabler/icons-react";

export default function Controls() {
  const dispatch = useDispatch();
  const store = useStore();
  const [restarting, setRestarting] = useState(false);
  const [restarted, setRestarted] = useState(false);
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

  const handleRestart = async () => {
    setRestarting(true);
    setRestarted(false);
    try {
      await restartService();
      setTimeout(async () => {
        await refreshStatus(dispatch);
        setRestarting(false);
        setRestarted(true);
        setTimeout(() => setRestarted(false), 3000);
      }, 2000);
    } catch (err) {
      console.error("Failed to restart service:", err);
      setRestarting(false);
    }
  };

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">Controls</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Adjust hardware settings in real time
        </p>
      </div>

      <div className="space-y-5">
        <div className="glass-card animate-stagger-in stagger-1 rounded-xl p-5">
          <div className="mb-5 flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-lg bg-amber-500/12 text-amber-500 dark:bg-amber-400/10 dark:text-amber-400">
              <IconKeyboard className="size-3.5" stroke={1.75} />
            </div>
            <div>
              <h3 className="text-[13px] font-semibold text-foreground">
                Keyboard Backlight
              </h3>
              <p className="text-[11px] text-muted-foreground">Adjust brightness level</p>
            </div>
          </div>
          <BacklightSlider />
        </div>

        <div className="glass-card animate-stagger-in stagger-2 rounded-xl p-5">
          <div className="mb-5 flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-lg bg-blue-500/12 text-blue-500 dark:bg-blue-400/10 dark:text-blue-400">
              <IconRotate className="size-3.5" stroke={1.75} />
            </div>
            <div>
              <h3 className="text-[13px] font-semibold text-foreground">
                Screen Orientation
              </h3>
              <p className="text-[11px] text-muted-foreground">
                Current: <span className="font-mono capitalize">{store.status.orientation}</span>
              </p>
            </div>
          </div>
          <OrientationButtons />
        </div>

        <div className="glass-card animate-stagger-in stagger-3 rounded-xl p-5">
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-2.5">
              <div className={cn(
                "flex size-7 items-center justify-center rounded-lg",
                store.status.serviceActive
                  ? "bg-emerald-500/12 text-emerald-500 dark:bg-emerald-400/10 dark:text-emerald-400"
                  : "bg-destructive/12 text-destructive"
              )}>
                <IconServer className="size-3.5" stroke={1.75} />
              </div>
              <div>
                <h3 className="text-[13px] font-semibold text-foreground">
                  Service Control
                </h3>
                <p className="text-[11px] text-muted-foreground">
                  Rust runtime is{" "}
                  <span className={cn(
                    "font-semibold",
                    store.status.serviceActive ? "text-emerald-500" : "text-destructive"
                  )}>
                    {store.status.serviceActive ? "running" : "stopped"}
                  </span>
                </p>
              </div>
            </div>
            <Button
              variant={restarted ? "outline" : "outline"}
              size="sm"
              onClick={handleRestart}
              disabled={restarting}
              className={cn(
                "gap-2 transition-all",
                restarted && "border-emerald-500/30 text-emerald-500"
              )}
            >
              {restarted ? (
                <IconCheck className="size-3.5" stroke={2} />
              ) : (
                <IconRefresh className={cn("size-3.5", restarting && "animate-spin")} stroke={1.5} />
              )}
              {restarting ? "Restarting..." : restarted ? "Restarted" : "Restart"}
            </Button>
          </div>
        </div>

        {touchscreens.length > 0 && (
          <div className="glass-card animate-stagger-in stagger-4 rounded-xl p-5">
            <div className="mb-5 flex items-center gap-2.5">
              <div className="flex size-7 items-center justify-center rounded-lg bg-purple-500/12 text-purple-500 dark:bg-purple-400/10 dark:text-purple-400">
                <IconHandFinger className="size-3.5" stroke={1.75} />
              </div>
              <div>
                <h3 className="text-[13px] font-semibold text-foreground">
                  Touchscreen
                </h3>
                <p className="text-[11px] text-muted-foreground">
                  Enable or disable touch input per display
                </p>
              </div>
            </div>
            <div className="space-y-3">
              {touchscreens.map((ts) => (
                <div key={ts.connector} className="flex items-center justify-between">
                  <span className="text-[13px]">
                    {ts.connector}
                    <span className="text-muted-foreground ml-2 text-[11px]">
                      {ts.name}
                    </span>
                  </span>
                  <Switch
                    checked={ts.enabled}
                    onCheckedChange={(checked) =>
                      handleTouchToggle(ts.connector, checked)
                    }
                  />
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
