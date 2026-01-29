import { useState, useMemo, useCallback } from "react";
import { useStore, useDispatch, refreshEvents } from "@/lib/store";
import EventStream from "@/components/EventStream";
import type { EventCategory, EventSeverity } from "@/types/duo";
import { Button } from "@/components/ui/button";
import {
  IconRefresh,
  IconDownload,
} from "@tabler/icons-react";
import { cn } from "@/lib/utils";

const ALL_CATEGORIES: EventCategory[] = [
  "USB",
  "DISPLAY",
  "KEYBOARD",
  "NETWORK",
  "ROTATION",
  "BLUETOOTH",
  "SERVICE",
];
const ALL_SEVERITIES: EventSeverity[] = ["info", "warning", "error"];

const severityStyles: Record<EventSeverity, { active: string; inactive: string }> = {
  info: {
    active: "bg-blue-500/15 text-blue-500 border-blue-500/30",
    inactive: "text-muted-foreground border-border hover:border-blue-500/30 hover:text-blue-500",
  },
  warning: {
    active: "bg-amber-500/15 text-amber-500 border-amber-500/30",
    inactive: "text-muted-foreground border-border hover:border-amber-500/30 hover:text-amber-500",
  },
  error: {
    active: "bg-red-500/15 text-red-500 border-red-500/30",
    inactive: "text-muted-foreground border-border hover:border-red-500/30 hover:text-red-500",
  },
};

export default function EventMonitor() {
  const store = useStore();
  const dispatch = useDispatch();
  const [categoryFilter, setCategoryFilter] = useState<Set<EventCategory>>(
    () => new Set(ALL_CATEGORIES)
  );
  const [severityFilter, setSeverityFilter] = useState<Set<EventSeverity>>(
    () => new Set(ALL_SEVERITIES)
  );

  const toggleCategory = useCallback((cat: EventCategory) => {
    setCategoryFilter((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  }, []);

  const toggleSeverity = useCallback((sev: EventSeverity) => {
    setSeverityFilter((prev) => {
      const next = new Set(prev);
      if (next.has(sev)) next.delete(sev);
      else next.add(sev);
      return next;
    });
  }, []);

  const filteredEvents = useMemo(
    () =>
      store.events.filter(
        (e) => categoryFilter.has(e.category) && severityFilter.has(e.severity)
      ),
    [store.events, categoryFilter, severityFilter]
  );

  const handleExport = () => {
    const data = JSON.stringify(store.events, null, 2);
    const blob = new Blob([data], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `duo-events-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div>
      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Event Monitor</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Real-time hardware event stream
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => refreshEvents(dispatch)} className="gap-1.5">
            <IconRefresh className="size-3.5" stroke={1.5} />
            Refresh
          </Button>
          <Button variant="outline" size="sm" onClick={handleExport} className="gap-1.5">
            <IconDownload className="size-3.5" stroke={1.5} />
            Export
          </Button>
        </div>
      </div>

      <div className="glass-card mb-4 rounded-xl p-5 animate-stagger-in stagger-1">
        <h3 className="mb-3 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          Filters
        </h3>

        <div className="space-y-3">
          <div>
            <span className="mb-2 block text-[12px] text-muted-foreground">Categories</span>
            <div className="flex flex-wrap gap-1.5">
              {ALL_CATEGORIES.map((cat) => (
                <button
                  key={cat}
                  className={cn(
                    "rounded-md border px-2.5 py-1 font-mono text-[11px] font-medium transition-all",
                    categoryFilter.has(cat)
                      ? "border-primary/30 bg-primary/10 text-primary"
                      : "border-border text-muted-foreground hover:border-primary/30 hover:text-primary"
                  )}
                  onClick={() => toggleCategory(cat)}
                >
                  {cat}
                </button>
              ))}
            </div>
          </div>
          <div>
            <span className="mb-2 block text-[12px] text-muted-foreground">Severity</span>
            <div className="flex flex-wrap gap-1.5">
              {ALL_SEVERITIES.map((sev) => {
                const styles = severityStyles[sev];
                return (
                  <button
                    key={sev}
                    className={cn(
                      "rounded-md border px-2.5 py-1 text-[11px] font-medium capitalize transition-all",
                      severityFilter.has(sev) ? styles.active : styles.inactive
                    )}
                    onClick={() => toggleSeverity(sev)}
                  >
                    {sev}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      </div>

      <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-2">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
            Event Stream
          </h3>
          <span className="font-mono text-[11px] text-muted-foreground">
            {filteredEvents.length} events
          </span>
        </div>
        <EventStream events={filteredEvents} />
      </div>
    </div>
  );
}
