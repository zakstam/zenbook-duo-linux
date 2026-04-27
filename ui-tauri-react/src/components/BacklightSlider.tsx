import { useRef, useState } from "react";
import { setBacklight } from "@/lib/tauri";
import { useStore, useDispatch } from "@/lib/store";
import { Slider } from "@/components/ui/slider";
import { cn } from "@/lib/utils";

const levels = [
  { value: 0, label: "Off", desc: "Backlight off" },
  { value: 1, label: "Low", desc: "Dim" },
  { value: 2, label: "Mid", desc: "Medium" },
  { value: 3, label: "Max", desc: "Full brightness" },
];

export default function BacklightSlider() {
  const store = useStore();
  const dispatch = useDispatch();
  const [pending, setPending] = useState(false);
  const [localLevel, setLocalLevel] = useState<number | null>(null);
  const clearTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const displayLevel = localLevel ?? store.status.backlightLevel;

  const handleChange = async (value: number[]) => {
    const level = value[0];
    if (level === undefined || level === displayLevel) return;

    if (clearTimer.current) clearTimeout(clearTimer.current);

    setLocalLevel(level);
    setPending(true);
    try {
      await setBacklight(level);
      dispatch({
        type: "SET_STATUS",
        payload: { ...store.status, backlightLevel: level },
      });
    } catch (err) {
      console.error("Failed to set backlight:", err);
    } finally {
      setPending(false);
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
            disabled={pending}
          >
            <span className="font-mono text-[12px] font-semibold">{l.label}</span>
            <span className="text-[10px] opacity-60">{l.desc}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
