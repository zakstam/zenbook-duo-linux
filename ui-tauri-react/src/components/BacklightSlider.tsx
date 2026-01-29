import { useRef, useState } from "react";
import { setBacklight } from "@/lib/tauri";
import { useStore, useDispatch } from "@/lib/store";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";

const levels = [
  { value: 0, label: "Off" },
  { value: 1, label: "Low" },
  { value: 2, label: "Mid" },
  { value: 3, label: "Max" },
];

export default function BacklightSlider() {
  const store = useStore();
  const dispatch = useDispatch();
  const [pending, setPending] = useState(false);
  const [localLevel, setLocalLevel] = useState<number | null>(null);
  const clearTimer = useRef<ReturnType<typeof setTimeout>>(null);

  const displayLevel = localLevel ?? store.status.backlightLevel;

  const handleChange = async (value: number[]) => {
    const level = value[0];
    if (level === displayLevel) return;

    // Clear any pending timer from a previous set
    if (clearTimer.current) clearTimeout(clearTimer.current);

    setLocalLevel(level);
    setPending(true);
    try {
      await setBacklight(level);
      // Directly update the store rather than re-reading the file,
      // which is subject to race conditions with the daemon/file watchers.
      dispatch({
        type: "SET_STATUS",
        payload: { ...store.status, backlightLevel: level },
      });
    } catch (err) {
      console.error("Failed to set backlight:", err);
    } finally {
      setPending(false);
      // Keep localLevel set briefly to override any stale file-watcher
      // refreshes that may arrive with an outdated value.
      clearTimer.current = setTimeout(() => setLocalLevel(null), 2000);
    }
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
          disabled={pending}
          className="flex-1"
        />
        <span className="font-mono text-[11px] text-muted-foreground">3</span>
      </div>
      <div className="flex justify-between">
        {levels.map((l) => (
          <button
            key={l.value}
            className={cn(
              "rounded-md px-3 py-1.5 font-mono text-[11px] font-medium transition-all",
              displayLevel === l.value
                ? "bg-primary/15 text-primary"
                : "text-muted-foreground hover:text-foreground"
            )}
            onClick={() => handleChange([l.value])}
            disabled={pending}
          >
            {l.label}
          </button>
        ))}
      </div>
    </div>
  );
}
