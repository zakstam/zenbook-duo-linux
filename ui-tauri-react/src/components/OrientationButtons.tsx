import { useState } from "react";
import { displayApi } from "@/lib/tauri-adapters";
import { useStore, useDispatch, refreshStatus } from "@/lib/store";
import type { Orientation } from "@/types/duo";
import { cn } from "@/lib/utils";
import {
  IconArrowUp,
  IconArrowLeft,
  IconArrowRight,
  IconArrowDown,
} from "@tabler/icons-react";

const orientations: { id: Orientation; label: string; icon: React.ComponentType<{ className?: string; stroke?: number }> }[] = [
  { id: "normal", label: "Normal", icon: IconArrowUp },
  { id: "left", label: "Left", icon: IconArrowLeft },
  { id: "right", label: "Right", icon: IconArrowRight },
  { id: "inverted", label: "Inverted", icon: IconArrowDown },
];

export default function OrientationButtons() {
  const store = useStore();
  const dispatch = useDispatch();
  const [pending, setPending] = useState(false);

  const handleClick = async (orientation: Orientation) => {
    setPending(true);
    try {
      await displayApi.setOrientation(orientation);
      await refreshStatus(dispatch);
    } catch (err) {
      console.error("Failed to set orientation:", err);
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="grid grid-cols-4 gap-2">
      {orientations.map((o) => {
        const Icon = o.icon;
        const isActive = store.status.orientation === o.id;
        return (
          <button
            key={o.id}
            className={cn(
              "group relative flex flex-col items-center gap-1.5 rounded-xl border px-3 py-3 transition-all",
              isActive
                ? "border-blue-500/30 bg-blue-500/8 text-blue-600 dark:text-blue-400"
                : "border-border text-muted-foreground hover:border-blue-500/20 hover:text-foreground"
            )}
            onClick={() => handleClick(o.id)}
            disabled={pending}
          >
            <div className={cn(
              "flex size-8 items-center justify-center rounded-lg transition-colors",
              isActive
                ? "bg-blue-500/15"
                : "bg-muted/50 group-hover:bg-muted"
            )}>
              <Icon className="size-4" stroke={isActive ? 2 : 1.5} />
            </div>
            <span className="text-[11px] font-medium">{o.label}</span>
          </button>
        );
      })}
    </div>
  );
}
