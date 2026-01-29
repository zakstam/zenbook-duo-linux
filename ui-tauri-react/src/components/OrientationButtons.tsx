import { useState } from "react";
import { setOrientation } from "@/lib/tauri";
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
      await setOrientation(orientation);
      await refreshStatus(dispatch);
    } catch (err) {
      console.error("Failed to set orientation:", err);
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="grid grid-cols-2 gap-2">
      {orientations.map((o) => {
        const Icon = o.icon;
        const isActive = store.status.orientation === o.id;
        return (
          <button
            key={o.id}
            className={cn(
              "flex items-center justify-center gap-2 rounded-lg border px-4 py-3 text-[13px] font-medium transition-all",
              isActive
                ? "border-primary/40 bg-primary/10 text-primary"
                : "border-border text-muted-foreground hover:border-primary/30 hover:text-foreground"
            )}
            onClick={() => handleClick(o.id)}
            disabled={pending}
          >
            <Icon className="size-4" stroke={1.5} />
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
