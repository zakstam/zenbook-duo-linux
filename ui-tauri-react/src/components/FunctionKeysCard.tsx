import { useState } from "react";
import { controlsApi } from "@/lib/tauri-adapters";
import { refreshStatus, useDispatch, useStoreSelector } from "@/lib/store";
import { autosavePersistedSetting } from "@/lib/settings-service";
import type { KeyOverride } from "@/types/duo";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { IconAlertTriangle, IconExternalLink, IconKeyboard } from "@tabler/icons-react";

// The top row's ASUS functions (what the keys do without Fn, or with Fn when
// the top row is in F1–F12 mode). XF86Launch1-4 can be bound to any action in
// the desktop's keyboard shortcut settings.
const TOP_ROW: { key: string; label: string; detail?: string }[] = [
  { key: "F1", label: "Mute" },
  { key: "F2", label: "Volume down" },
  { key: "F3", label: "Volume up" },
  { key: "F4", label: "Keyboard backlight" },
  { key: "F5", label: "Brightness down" },
  { key: "F6", label: "Brightness up" },
  { key: "F7", label: "Share", detail: "Super+P (the GNOME extension opens its display controls)" },
  { key: "F8", label: "Screen arrows", detail: "XF86Launch2" },
  { key: "F9", label: "Mic mute", detail: "Hold it and press Up/Down or the volume keys for the mic volume" },
  { key: "F10", label: "Bluetooth", detail: "The keyboard's own pairing button; sends nothing to the computer" },
  { key: "F11", label: "Emoji picker" },
  { key: "F12", label: "ASUS hub", detail: "Opens this Control Panel (MyASUS on Windows)" },
  { key: "Screen", label: "Screen key", detail: "Right of F12: cycles the screens" },
];

type OverrideKind = KeyOverride["action"];

// Keys GNOME has no use for, one per top-row key, for GNOME's own keyboard
// shortcuts (the same list as the daemon's SHORTCUT_SIGNALS).
const SHORTCUT_SIGNALS = [
  "Launch1", "Launch3", "Launch4", "Launch5", "Launch6", "Launch7",
  "Launch8", "Launch9", "LaunchA", "LaunchB", "F19", "F24",
];

/** How GNOME shows a signal when you press the key in its shortcut settings. */
function gnomeName(signal: string) {
  return signal.startsWith("Launch") ? `XF86${signal}` : signal;
}

/** The text a command or key-combination override carries. */
function overrideText(over: KeyOverride | undefined) {
  if (over?.action === "command") return over.command;
  if (over?.action === "keys") return over.keys;
  return "";
}

function KeyOverrideRow({
  fkey,
  label,
  detail,
  value,
  onChange,
  disabled,
  takenSignals,
  onOpenShortcuts,
}: {
  fkey: string;
  label: string;
  detail?: string;
  value: KeyOverride | undefined;
  onChange: (next: KeyOverride) => void;
  disabled: boolean;
  /** Shortcut signals other keys use, with the key using each. */
  takenSignals: Map<string, string>;
  onOpenShortcuts: () => void;
}) {
  const kind: OverrideKind = value?.action ?? "default";
  // The parent remounts the row when the saved text changes, so this starts fresh.
  const [text, setText] = useState(overrideText(value));
  const isFKey = kind === "fKey";
  // The screen key has no F-key of its own.
  const hasFKey = fkey !== "Screen";

  const choose = (next: OverrideKind) => {
    if (next === "shortcut") {
      // The first signal no other key uses.
      const signal = SHORTCUT_SIGNALS.find((s) => !takenSignals.has(s));
      if (signal) onChange({ action: "shortcut", signal });
      return;
    }
    if (next === "command") onChange({ action: "command", command: text });
    else if (next === "keys") onChange({ action: "keys", keys: text });
    else onChange({ action: next } as KeyOverride);
  };
  const commitText = () => {
    if (text === overrideText(value)) return;
    if (kind === "command") onChange({ action: "command", command: text });
    if (kind === "keys") onChange({ action: "keys", keys: text });
  };

  return (
    <div className="space-y-1.5 py-1.5">
      <div className="flex items-center justify-between gap-3 text-[12px]">
        <span className="min-w-0">
          <span className="font-mono">{fkey}</span>
          <span className="ml-2">{label}</span>
          {detail && <span className="ml-2 text-muted-foreground">{detail}</span>}
        </span>
        <div className="flex shrink-0 items-center gap-1.5">
          {/* Quick switch: this key sends its own F-key instead. */}
          {hasFKey && (
            <Button
              variant={isFKey ? "default" : "outline"}
              size="sm"
              className="h-7 w-12 px-0 font-mono text-[11px]"
              title={isFKey ? `Back to ${label}` : `Send ${fkey} instead of ${label}`}
              onClick={() => choose(isFKey ? "default" : "fKey")}
              disabled={disabled}
            >
              {fkey}
            </Button>
          )}
          <Select value={kind} onValueChange={(v) => choose(v as OverrideKind)} disabled={disabled}>
            <SelectTrigger className="h-7 w-40 text-[12px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">{label}</SelectItem>
              {hasFKey && <SelectItem value="fKey">{fkey} key</SelectItem>}
              <SelectItem value="command">Run a command…</SelectItem>
              <SelectItem value="keys">Send keys…</SelectItem>
              <SelectItem value="shortcut">GNOME shortcut</SelectItem>
              <SelectItem value="nothing">Nothing</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      {value?.action === "shortcut" && (
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
          <span className="min-w-0 flex-1">
            In Keyboard shortcuts, give any action (or a custom shortcut) a new shortcut and press{" "}
            {hasFKey ? fkey : "the screen key"} when asked (GNOME shows{" "}
            <span className="font-mono">{gnomeName(value.signal)}</span>). If another app reacts to it,
            pick another key:
          </span>
          <Select
            value={value.signal}
            onValueChange={(signal) => onChange({ action: "shortcut", signal })}
            disabled={disabled}
          >
            <SelectTrigger className="h-7 w-32 shrink-0 font-mono text-[11px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SHORTCUT_SIGNALS.map((signal) => (
                <SelectItem
                  key={signal}
                  value={signal}
                  disabled={takenSignals.has(signal)}
                  className="font-mono text-[11px]"
                >
                  {signal}
                  {takenSignals.has(signal) ? ` (${takenSignals.get(signal)})` : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="sm"
            className="h-7 shrink-0 px-2"
            title="Open keyboard shortcuts"
            onClick={onOpenShortcuts}
          >
            <IconExternalLink className="size-3.5" stroke={1.5} />
          </Button>
        </div>
      )}
      {(kind === "command" || kind === "keys") && (
        <Input
          className="h-7 text-[12px]"
          placeholder={kind === "command" ? "e.g. gnome-calculator" : "e.g. Ctrl+Alt+T, Super+E or Launch5"}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={commitText}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitText();
          }}
          disabled={disabled}
        />
      )}
    </div>
  );
}

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
  const overrides = useStoreSelector((s) => s.settings.keyOverrides) ?? {};

  /** The shortcut signals the other keys use, by signal → key. */
  const signalsTakenBesides = (fkey: string) =>
    new Map(
      Object.entries(overrides)
        .filter(([key, over]) => key !== fkey && over.action === "shortcut")
        .map(([key, over]) => [(over as { signal: string }).signal, key]),
    );

  const handleOverride = async (fkey: string, next: KeyOverride) => {
    setPending(true);
    setError(null);
    try {
      const updated = { ...overrides };
      if (next.action === "default") delete updated[fkey];
      else updated[fkey] = next;
      await autosavePersistedSetting("keyOverrides", updated, dispatch);
    } catch (err) {
      setError(errorMessage(err, `Failed to change ${fkey}`));
    } finally {
      setPending(false);
    }
  };

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

  const handleOpenShortcuts = async () => {
    setError(null);
    try {
      await controlsApi.openKeyboardShortcuts();
    } catch (err) {
      setError(errorMessage(err, "Failed to open keyboard shortcuts"));
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

      <div className="mt-5 border-t border-border/50 pt-4">
        <p className="mb-1 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          Top-row keys
        </p>
        <p className="mb-2 text-[11px] text-muted-foreground">
          Change what a key does in place of its ASUS function. The F-key itself (with Fn) stays the
          same. Commands run as you, never as root.
        </p>
        <div className="divide-y divide-border/40">
          {TOP_ROW.map(({ key, label, detail }) => (
            <KeyOverrideRow
              key={`${key}:${overrideText(overrides[key])}`}
              fkey={key}
              label={label}
              detail={detail}
              value={overrides[key]}
              onChange={(next) => void handleOverride(key, next)}
              disabled={pending}
              takenSignals={signalsTakenBesides(key)}
              onOpenShortcuts={() => void handleOpenShortcuts()}
            />
          ))}
        </div>

        <div className="mt-4 flex items-center justify-between gap-4">
          <p className="text-[11px] text-muted-foreground">
            F8 sends XF86Launch2. To bind a key in your keyboard shortcuts instead, set it to send Launch1–9.
          </p>
          <Button
            variant="outline"
            size="sm"
            className="shrink-0 gap-2"
            onClick={() => void handleOpenShortcuts()}
          >
            <IconExternalLink className="size-3.5" stroke={1.5} />
            Keyboard shortcuts
          </Button>
        </div>
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
