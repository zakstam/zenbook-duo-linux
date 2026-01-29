import { useState, useMemo } from "react";
import { useStore, useDispatch, refreshLogs, clearLogs } from "@/lib/store";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { IconRefresh, IconFilter, IconFileText, IconTrash, IconCopy, IconCheck } from "@tabler/icons-react";

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

  const handleClear = () => {
    clearLogs(dispatch);
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
            System log output from zenbook-duo-user
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span className="font-mono text-[12px] text-muted-foreground">
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

      <div className="glass-card rounded-xl p-4 animate-stagger-in stagger-2">
        {filteredLogs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <div className="mb-3 flex size-10 items-center justify-center rounded-lg bg-muted">
              <IconFileText className="size-4 text-muted-foreground" stroke={1.5} />
            </div>
            <p className="text-sm text-muted-foreground">No log entries found</p>
          </div>
        ) : (
          <pre className="max-h-[500px] overflow-auto whitespace-pre-wrap font-mono text-[12px] leading-relaxed text-foreground/90">
            {filteredLogs.join("\n")}
          </pre>
        )}
      </div>
    </div>
  );
}
