import { useState, useMemo } from "react";
import { useStore, useDispatch, refreshLogs, clearLogs } from "@/lib/store";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { IconRefresh, IconFilter, IconFileText, IconTrash, IconCopy, IconCheck } from "@tabler/icons-react";

function getLogLevel(line: string): "error" | "warn" | "info" | null {
  const lower = line.toLowerCase();
  if (lower.includes("error") || lower.includes("fatal") || lower.includes("panic")) return "error";
  if (lower.includes("warn")) return "warn";
  return null;
}

const logLevelStyles = {
  error: "text-red-400",
  warn: "text-amber-400",
  info: "",
} as const;

export default function Logs() {
  const store = useStore();
  const dispatch = useDispatch();
  const [filter, setFilter] = useState("");
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(filteredLogs.join("\n"));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleClear = async () => {
    await clearLogs(dispatch);
  };

  const filteredLogs = useMemo(() => {
    const f = filter.toLowerCase();
    if (!f) return store.logs;
    return store.logs.filter((line) => line.toLowerCase().includes(f));
  }, [filter, store.logs]);

  return (
    <div>
      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Logs</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            System log output from the Rust runtime
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded-md bg-muted px-2 py-1 font-mono text-[11px] tabular-nums text-muted-foreground">
            {filteredLogs.length} entries
          </span>
          <Button variant="outline" size="sm" onClick={handleCopy} className="gap-1.5" disabled={filteredLogs.length === 0}>
            {copied ? <IconCheck className="size-3.5" stroke={1.5} /> : <IconCopy className="size-3.5" stroke={1.5} />}
            {copied ? "Copied" : "Copy"}
          </Button>
          <Button variant="outline" size="sm" onClick={handleClear} className="gap-1.5" disabled={store.logs.length === 0}>
            <IconTrash className="size-3.5" stroke={1.5} />
            Clear
          </Button>
          <Button variant="outline" size="sm" onClick={() => refreshLogs(dispatch)} className="gap-1.5">
            <IconRefresh className="size-3.5" stroke={1.5} />
            Refresh
          </Button>
        </div>
      </div>

      <div className="mb-4 animate-stagger-in stagger-1">
        <div className="relative">
          <IconFilter className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" stroke={1.5} />
          <Input
            type="text"
            placeholder="Filter logs..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="pl-9"
          />
        </div>
      </div>

      <div className="glass-card rounded-xl p-1 animate-stagger-in stagger-2">
        {filteredLogs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <div className="mb-3 flex size-10 items-center justify-center rounded-lg bg-muted">
              <IconFileText className="size-4 text-muted-foreground" stroke={1.5} />
            </div>
            <p className="text-sm text-muted-foreground">No log entries found</p>
          </div>
        ) : (
          <div className="max-h-[500px] overflow-auto rounded-lg bg-black/30 dark:bg-black/40">
            <table className="w-full">
              <tbody className="font-mono text-[12px] leading-relaxed">
                {filteredLogs.map((line, i) => {
                  const level = getLogLevel(line);
                  return (
                    <tr key={i} className={cn(
                      "group border-b border-transparent transition-colors hover:bg-white/5",
                      level === "error" && "bg-red-500/5",
                      level === "warn" && "bg-amber-500/5"
                    )}>
                      <td className="select-none whitespace-nowrap py-0.5 pl-3 pr-3 text-right text-[10px] tabular-nums text-muted-foreground/40">
                        {i + 1}
                      </td>
                      <td className={cn(
                        "whitespace-pre-wrap py-0.5 pr-3 text-foreground/90",
                        level && logLevelStyles[level]
                      )}>
                        {line}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
