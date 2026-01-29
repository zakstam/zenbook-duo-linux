import type { HardwareEvent } from "@/types/duo";
import { cn } from "@/lib/utils";
import { IconBolt } from "@tabler/icons-react";

interface EventStreamProps {
  events: HardwareEvent[];
}

const severityColor: Record<string, string> = {
  info: "text-foreground/80",
  warning: "text-amber-500",
  error: "text-red-500",
};

const categoryColor: Record<string, string> = {
  USB: "bg-violet-500/15 text-violet-500",
  DISPLAY: "bg-blue-500/15 text-blue-500",
  KEYBOARD: "bg-emerald-500/15 text-emerald-500",
  NETWORK: "bg-cyan-500/15 text-cyan-500",
  ROTATION: "bg-orange-500/15 text-orange-500",
  BLUETOOTH: "bg-indigo-500/15 text-indigo-500",
  SERVICE: "bg-pink-500/15 text-pink-500",
};

function formatTime(timestamp: string) {
  try {
    const date = new Date(timestamp);
    return date.toLocaleTimeString("en-US", { hour12: false });
  } catch {
    return "??:??:??";
  }
}

export default function EventStream({ events }: EventStreamProps) {
  if (events.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-10 text-center">
        <div className="mb-3 flex size-10 items-center justify-center rounded-lg bg-muted">
          <IconBolt className="size-4 text-muted-foreground" stroke={1.5} />
        </div>
        <p className="text-sm text-muted-foreground">No events recorded yet</p>
      </div>
    );
  }

  return (
    <div className="space-y-0.5">
      {events.map((event, i) => (
        <div
          key={`${event.timestamp}-${i}`}
          className="flex items-center gap-3 rounded-md px-2 py-1.5 transition-colors hover:bg-muted/40"
        >
          <span className="shrink-0 font-mono text-[11px] text-muted-foreground/70">
            {formatTime(event.timestamp)}
          </span>
          <span
            className={cn(
              "shrink-0 rounded px-1.5 py-0.5 font-mono text-[10px] font-medium",
              categoryColor[event.category] ?? "bg-muted text-muted-foreground"
            )}
          >
            {event.category}
          </span>
          <span className={cn("truncate text-[12px]", severityColor[event.severity])}>
            {event.message}
          </span>
        </div>
      ))}
    </div>
  );
}
