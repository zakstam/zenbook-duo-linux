import { useState } from "react";
import { controlsApi } from "@/lib/tauri-adapters";
import { refreshStatus, useDispatch, useStoreSelector } from "@/lib/store";
import { Switch } from "@/components/ui/switch";
import { IconAlertTriangle, IconKeyboard } from "@tabler/icons-react";

function errorMessage(err: unknown, fallback: string) {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message?: unknown }).message);
  }
  return fallback;
}

export default function FunctionKeysCard() {
  const dispatch = useDispatch();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fnLock = useStoreSelector((s) => s.status.fnLock);

  const handleFnLock = async (locked: boolean) => {
    setPending(true);
    setError(null);
    try {
      await controlsApi.setFnLock(locked);
      await refreshStatus(dispatch);
    } catch (err) {
      setError(errorMessage(err, "Failed to change the top row"));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="glass-card rounded-xl p-5">
      <div className="mb-5 flex items-center gap-2.5">
        <div className="flex size-7 items-center justify-center rounded-lg bg-sky-500/12 text-sky-500 dark:bg-sky-400/10 dark:text-sky-400">
          <IconKeyboard className="size-3.5" stroke={1.75} />
        </div>
        <div>
          <h3 className="text-[13px] font-semibold text-foreground">Function Keys</h3>
          <p className="text-[11px] text-muted-foreground">The top row and the ASUS keys</p>
        </div>
      </div>

      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-[13px]">Top row sends F1–F12</p>
          <p className="text-[11px] text-muted-foreground">
            {fnLock
              ? "Hold Fn for media keys"
              : "Now media keys · hold Fn for F1–F12"}
            {" · Fn+Esc switches"}
          </p>
        </div>
        <Switch
          checked={fnLock}
          onCheckedChange={(checked) => void handleFnLock(checked)}
          disabled={pending}
        />
      </div>

      {error && (
        <div className="mt-4 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-[12px] text-destructive">
          <IconAlertTriangle className="size-3.5 shrink-0" stroke={1.5} />
          <span>{error}</span>
        </div>
      )}
    </div>
  );
}
