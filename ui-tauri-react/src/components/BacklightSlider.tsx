import { useEffect, useRef, useState } from "react";
import { controlsApi } from "@/lib/tauri-adapters";
import { useStoreSelector, useDispatch } from "@/lib/store";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";

const levels = [
  { value: 0, label: "Off", desc: "Backlight off" },
  { value: 1, label: "Low", desc: "Dim" },
  { value: 2, label: "Mid", desc: "Medium" },
  { value: 3, label: "Max", desc: "Full brightness" },
];

export default function BacklightSlider() {
  const status = useStoreSelector((s) => s.status);
  const dispatch = useDispatch();
  // The level picked here, shown until the daemon's status agrees with it.
  const [picked, setPicked] = useState<number | null>(null);
  const inFlight = useRef(false);
  const queued = useRef<number | null>(null);

  const actual = status.backlightLevel;
  const displayLevel = picked ?? actual;

  // The status caught up (or the keyboard changed it, e.g. Fn+F4): follow it.
  useEffect(() => {
    if (picked !== null && !inFlight.current && actual === picked) setPicked(null);
  }, [actual, picked]);

  // One request at a time; the last level picked always gets sent.
  const send = async (level: number) => {
    if (inFlight.current) {
      queued.current = level;
      return;
    }
    inFlight.current = true;
    try {
      await controlsApi.setBacklight(level);
      dispatch({
        type: "SET_STATUS",
        payload: { ...status, backlightLevel: level },
      });
    } catch (err) {
      console.error("Failed to set backlight:", err);
      setPicked(null);
    } finally {
      inFlight.current = false;
      const next = queued.current;
      queued.current = null;
      if (next !== null && next !== level) void send(next);
    }
  };

  const handleChange = (value: number[]) => {
    const level = value[0];
    if (level === undefined || level === displayLevel) return;
    setPicked(level);
    void send(level);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-4">
        <span className="font-mono text-[11px] text-muted-foreground">0</span>
        <Slider
          min={0}
          max={3}
          step={1}
          value={[displayLevel]}
          onValueChange={handleChange}
          className="flex-1"
        />
        <span className="font-mono text-[11px] text-muted-foreground">3</span>
      </div>
      <div className="grid grid-cols-4 gap-1.5">
        {levels.map((l) => (
          <button
            key={l.value}
            className={cn(
              "flex flex-col items-center gap-0.5 rounded-lg border px-2 py-2 transition-all",
              displayLevel === l.value
                ? "border-amber-500/30 bg-amber-500/8 text-amber-600 dark:text-amber-400"
                : "border-transparent text-muted-foreground hover:border-border hover:text-foreground"
            )}
            onClick={() => handleChange([l.value])}
          >
            <span className="font-mono text-[12px] font-semibold">{l.label}</span>
            <span className="text-[10px] opacity-60">{l.desc}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
