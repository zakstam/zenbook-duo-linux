import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import * as api from "@/lib/tauri";
import type {
  EvdevDevice,
  EvdevEvent,
  EvdevEventMulti,
  HidDevice,
  ReportDescriptor,
  HidrawCapture,
} from "@/types/duo";
import {
  IconBug,
  IconPlayerPlay,
  IconRefresh,
  IconDownload,
  IconDeviceGamepad,
  IconTopologyStar3,
  IconTerminal,
} from "@tabler/icons-react";

function downloadJson(filename: string, data: unknown) {
  const blob = new Blob([JSON.stringify(data, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export default function Diagnostics() {
  const [evdev, setEvdev] = useState<EvdevDevice[]>([]);
  const [evdevLoading, setEvdevLoading] = useState(false);
  const [selectedEvdev, setSelectedEvdev] = useState<string>("/dev/input/event28");
  const [evdevSeconds, setEvdevSeconds] = useState(5);
  const [evdevEvents, setEvdevEvents] = useState<EvdevEvent[]>([]);
  const [evdevEventsMulti, setEvdevEventsMulti] = useState<EvdevEventMulti[]>([]);
  const [evdevError, setEvdevError] = useState<string | null>(null);

  const [vid, setVid] = useState("0b05");
  const [pid, setPid] = useState("1b2c");
  const [hid, setHid] = useState<HidDevice[]>([]);
  const [hidLoading, setHidLoading] = useState(false);
  const [selectedHidId, setSelectedHidId] = useState<string>("");
  const [descriptor, setDescriptor] = useState<ReportDescriptor | null>(null);
  const [descriptorLoading, setDescriptorLoading] = useState(false);
  const [descriptorError, setDescriptorError] = useState<string | null>(null);

  const hidrawNodes = useMemo(() => {
    const s = new Set<string>();
    for (const d of hid) {
      for (const n of d.hidrawNodes) s.add(n);
    }
    return Array.from(s).sort();
  }, [hid]);

  const [selectedHidraw, setSelectedHidraw] = useState<string>("");
  const [hidrawSeconds, setHidrawSeconds] = useState(5);
  const [hidrawCapture, setHidrawCapture] = useState<HidrawCapture | null>(null);
  const [hidrawLoading, setHidrawLoading] = useState(false);
  const [hidrawError, setHidrawError] = useState<string | null>(null);

  async function refreshEvdev() {
    setEvdevLoading(true);
    try {
      const devices = await api.diagListEvdev();
      setEvdev(devices);
      setEvdevError(null);
      const firstDevice = devices[0];
      if (!devices.some((d) => d.eventPath === selectedEvdev) && firstDevice) {
        setSelectedEvdev(firstDevice.eventPath);
      }
    } catch (e) {
      setEvdevError(String(e));
    } finally {
      setEvdevLoading(false);
    }
  }

  async function refreshHid() {
    setHidLoading(true);
    try {
      const devices = await api.diagListHid(vid, pid);
      setHid(devices);
      const firstDevice = devices[0];
      if (!devices.some((d) => d.id === selectedHidId)) setSelectedHidId(firstDevice?.id ?? "");

      const nextHidraw = Array.from(
        new Set(devices.flatMap((d) => d.hidrawNodes))
      ).sort();
      const firstHidraw = nextHidraw[0];
      if (firstHidraw && !nextHidraw.includes(selectedHidraw)) {
        setSelectedHidraw(firstHidraw);
      }
      setDescriptor(null);
      setDescriptorError(null);
    } catch (e) {
      setDescriptorError(String(e));
    } finally {
      setHidLoading(false);
    }
  }

  useEffect(() => {
    refreshEvdev();
    refreshHid();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function captureEvdev() {
    setEvdevError(null);
    setEvdevEvents([]);
    setEvdevEventsMulti([]);
    try {
      const events = await api.diagCaptureEvdev(selectedEvdev, evdevSeconds);
      setEvdevEvents(events);
    } catch (e) {
      setEvdevError(String(e));
    }
  }

  async function captureEvdevAllKeyboard() {
    setEvdevError(null);
    setEvdevEvents([]);
    setEvdevEventsMulti([]);
    try {
      const paths = evdev
        .filter((d) =>
          d.name.includes("Zenbook Duo Keyboard") &&
          (d.vendor ?? "").toLowerCase() === vid.toLowerCase() &&
          (d.product ?? "").toLowerCase() === pid.toLowerCase()
        )
        .map((d) => d.eventPath);
      const events = await api.diagCaptureEvdevMulti(paths, evdevSeconds);
      setEvdevEventsMulti(events);
    } catch (e) {
      setEvdevError(String(e));
    }
  }

  async function loadDescriptor() {
    if (!selectedHidId) return;
    setDescriptorLoading(true);
    setDescriptor(null);
    setDescriptorError(null);
    try {
      const d = await api.diagReadReportDescriptor(selectedHidId);
      setDescriptor(d);
    } catch (e) {
      setDescriptorError(String(e));
    } finally {
      setDescriptorLoading(false);
    }
  }

  async function captureHidraw() {
    if (!selectedHidraw) return;
    setHidrawLoading(true);
    setHidrawCapture(null);
    setHidrawError(null);
    try {
      const cap = await api.diagCaptureHidrawPkexec(selectedHidraw, hidrawSeconds);
      setHidrawCapture(cap);
    } catch (e) {
      setHidrawError(String(e));
    } finally {
      setHidrawLoading(false);
    }
  }

  const evdevSelected = useMemo(
    () => evdev.find((d) => d.eventPath === selectedEvdev) ?? null,
    [evdev, selectedEvdev]
  );

  const hidSelected = useMemo(
    () => hid.find((d) => d.id === selectedHidId) ?? null,
    [hid, selectedHidId]
  );

  // Convenience: when selecting a HID interface, preselect its mapped nodes.
  useEffect(() => {
    if (!hidSelected) return;
    const nextEvdev = hidSelected.inputEventNodes[0];
    if (nextEvdev && nextEvdev !== selectedEvdev) setSelectedEvdev(nextEvdev);

    const nextHidraw = hidSelected.hidrawNodes[0];
    if (nextHidraw && nextHidraw !== selectedHidraw) setSelectedHidraw(nextHidraw);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hidSelected?.id]);

  return (
    <div>
      <div className="mb-6">
        <div className="flex items-center gap-2">
          <IconBug className="size-5 text-primary" stroke={1.75} />
          <h1 className="text-xl font-semibold tracking-tight">Diagnostics</h1>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          One-time input debugging (evdev + HID descriptors + hidraw via pkexec)
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4">
        {/* Evdev Capture */}
        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-1">
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-2.5">
              <div className="flex size-7 items-center justify-center rounded-lg bg-violet-500/12 text-violet-500 dark:bg-violet-400/10 dark:text-violet-400">
                <IconDeviceGamepad className="size-3.5" stroke={1.75} />
              </div>
              <h3 className="text-[13px] font-semibold text-foreground">
                Evdev Capture
              </h3>
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={refreshEvdev}
                className="gap-1.5"
                disabled={evdevLoading}
              >
                <IconRefresh className="size-3.5" stroke={1.5} />
                Refresh
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  downloadJson(
                    `evdev-${new Date().toISOString().slice(0, 19)}.json`,
                    {
                      device: evdevSelected,
                      events: evdevEvents,
                      multiEvents: evdevEventsMulti,
                    }
                  )
                }
                className="gap-1.5"
                disabled={evdevEvents.length === 0 && evdevEventsMulti.length === 0}
              >
                <IconDownload className="size-3.5" stroke={1.5} />
                Export
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <Select value={selectedEvdev} onValueChange={setSelectedEvdev}>
                <SelectTrigger className="h-8 w-auto min-w-[220px] font-mono text-xs">
                  <SelectValue placeholder="Select device" />
                </SelectTrigger>
                <SelectContent>
                  {evdev.map((d) => (
                    <SelectItem key={d.eventPath} value={d.eventPath} className="font-mono text-xs">
                      {d.eventPath} - {d.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Select value={String(evdevSeconds)} onValueChange={(v) => setEvdevSeconds(Number(v))}>
                <SelectTrigger className="h-8 w-[70px] font-mono text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {[3, 5, 10, 15].map((s) => (
                    <SelectItem key={s} value={String(s)} className="font-mono text-xs">
                      {s}s
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              <Button
                size="sm"
                onClick={captureEvdev}
                className="gap-1.5"
                disabled={!selectedEvdev}
              >
                <IconPlayerPlay className="size-3.5" stroke={1.5} />
                Capture
              </Button>

              <Button
                variant="outline"
                size="sm"
                onClick={captureEvdevAllKeyboard}
                className="gap-1.5"
                disabled={evdev.length === 0}
              >
                Capture all keyboard nodes
              </Button>
            </div>

            {evdevSelected && (
              <div className="flex flex-wrap items-center gap-2 rounded-lg bg-muted/30 px-3 py-1.5 text-xs text-muted-foreground">
                <span><span className="font-mono font-medium">phys</span> {evdevSelected.phys ?? "-"}</span>
                <span className="text-border">|</span>
                <span><span className="font-mono font-medium">id</span> {(evdevSelected.vendor ?? "??") + ":" + (evdevSelected.product ?? "??")}</span>
              </div>
            )}

            {evdevError && (
              <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
                {evdevError}
              </div>
            )}

            <pre className={cn(
              "max-h-[280px] overflow-auto rounded-lg border border-border/60 bg-black/30 dark:bg-black/40 p-3 font-mono text-[11px] leading-relaxed",
              (evdevEvents.length === 0 && evdevEventsMulti.length === 0)
                ? "text-muted-foreground"
                : "text-foreground"
            )}>
              {evdevEvents.length === 0 && evdevEventsMulti.length === 0
                ? "No events captured yet. Press Fn, F1, Fn+F1, etc during capture."
                : evdevEventsMulti.length > 0
                  ? evdevEventsMulti
                      .map(
                        (e) =>
                          `${e.eventPath} ${String(e.tsSec).padStart(10)}.${String(e.tsUsec).padStart(6, "0")} type=${e.typeCode} code=${e.code} value=${e.value}`
                      )
                      .join("\n")
                  : evdevEvents
                      .map(
                        (e) =>
                          `${String(e.tsSec).padStart(10)}.${String(e.tsUsec).padStart(6, "0")} type=${e.typeCode} code=${e.code} value=${e.value}`
                      )
                      .join("\n")}
            </pre>
          </div>
        </div>

        {/* HID Topology */}
        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-2">
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-2.5">
              <div className="flex size-7 items-center justify-center rounded-lg bg-cyan-500/12 text-cyan-500 dark:bg-cyan-400/10 dark:text-cyan-400">
                <IconTopologyStar3 className="size-3.5" stroke={1.75} />
              </div>
              <h3 className="text-[13px] font-semibold text-foreground">
                HID Topology
              </h3>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={refreshHid}
              className="gap-1.5"
              disabled={hidLoading}
            >
              <IconRefresh className="size-3.5" stroke={1.5} />
              Refresh
            </Button>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-medium text-muted-foreground">VID</span>
              <input
                className="h-8 w-[90px] rounded-md border border-border bg-background px-2 font-mono text-xs"
                value={vid}
                onChange={(e) => setVid(e.target.value)}
              />
            </div>
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-medium text-muted-foreground">PID</span>
              <input
                className="h-8 w-[90px] rounded-md border border-border bg-background px-2 font-mono text-xs"
                value={pid}
                onChange={(e) => setPid(e.target.value)}
              />
            </div>
            <Button size="sm" onClick={refreshHid} className="gap-1.5">
              <IconRefresh className="size-3.5" stroke={1.5} />
              Load
            </Button>
          </div>

          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
            <div className="rounded-lg border border-border/60 bg-muted/20 p-3">
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                Devices
              </div>
              <Select value={selectedHidId} onValueChange={setSelectedHidId}>
                <SelectTrigger className="h-8 w-full font-mono text-xs">
                  <SelectValue placeholder="Select device" />
                </SelectTrigger>
                <SelectContent>
                  {hid.map((d) => (
                    <SelectItem key={d.id} value={d.id} className="font-mono text-xs">
                      {d.id} ({d.driver ?? "?"})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <div className="mt-2 space-y-0.5 font-mono text-[11px] text-muted-foreground">
                <div><span className="text-muted-foreground/60">hidName:</span> {hidSelected?.hidName ?? "-"}</div>
                <div><span className="text-muted-foreground/60">hidPhys:</span> {hidSelected?.hidPhys ?? "-"}</div>
                <div><span className="text-muted-foreground/60">hidId:</span> {hidSelected?.hidId ?? "-"}</div>
              </div>
              <p className="mt-2 rounded-lg bg-muted/30 px-2.5 py-2 text-[11px] leading-relaxed text-muted-foreground">
                The keyboard exposes multiple HID interfaces, each mapping to different Linux devices
                (evdev + hidraw). Pick the interface that lists the evdev node you care about, then
                capture both evdev and its corresponding hidraw.
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={loadDescriptor}
                  disabled={!selectedHidId || descriptorLoading}
                  className="gap-1.5"
                >
                  Read report_descriptor
                </Button>
              </div>
            </div>

            <div className="rounded-lg border border-border/60 bg-muted/20 p-3">
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                Mappings
              </div>
              <div className="space-y-3 font-mono text-[11px]">
                <div>
                  <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">hidraw</div>
                  <div className="rounded-md bg-muted/30 px-2 py-1 text-foreground">
                    {(hidSelected?.hidrawNodes ?? []).join(" ") || "-"}
                  </div>
                </div>
                <div>
                  <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">evdev</div>
                  <div className="rounded-md bg-muted/30 px-2 py-1 text-foreground">
                    {(hidSelected?.inputEventNodes ?? []).join(" ") || "-"}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {descriptorError && (
            <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
              {descriptorError}
            </div>
          )}

          {descriptor && (
            <div className="mt-3 rounded-lg border border-border/60 bg-black/30 dark:bg-black/40 p-3">
              <div className="mb-2 flex items-center justify-between">
                <span className="font-mono text-[11px] text-muted-foreground">
                  len={descriptor.len} reportIds=[{descriptor.reportIds.join(", ")}]
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    downloadJson(
                      `report-descriptor-${selectedHidId}.json`,
                      descriptor
                    )
                  }
                  disabled={!descriptor}
                  className="gap-1.5"
                >
                  <IconDownload className="size-3.5" stroke={1.5} />
                  Export
                </Button>
              </div>
              <pre className="max-h-[240px] overflow-auto font-mono text-[11px] text-foreground">
                {descriptor.hex}
              </pre>
            </div>
          )}
        </div>

        {/* Hidraw Capture */}
        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-3">
          <div className="mb-4 flex items-center justify-between">
            <div className="flex items-center gap-2.5">
              <div className="flex size-7 items-center justify-center rounded-lg bg-orange-500/12 text-orange-500 dark:bg-orange-400/10 dark:text-orange-400">
                <IconTerminal className="size-3.5" stroke={1.75} />
              </div>
              <h3 className="text-[13px] font-semibold text-foreground">
                Hidraw Capture
              </h3>
              <span className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">pkexec</span>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                downloadJson(
                  `hidraw-${new Date().toISOString().slice(0, 19)}.json`,
                  hidrawCapture
                )
              }
              className="gap-1.5"
              disabled={!hidrawCapture}
            >
              <IconDownload className="size-3.5" stroke={1.5} />
              Export
            </Button>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Select value={selectedHidraw} onValueChange={setSelectedHidraw}>
              <SelectTrigger className="h-8 w-[160px] font-mono text-xs">
                <SelectValue placeholder="Select hidraw" />
              </SelectTrigger>
              <SelectContent>
                {hidrawNodes.map((n) => (
                  <SelectItem key={n} value={n} className="font-mono text-xs">
                    {n}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Select value={String(hidrawSeconds)} onValueChange={(v) => setHidrawSeconds(Number(v))}>
              <SelectTrigger className="h-8 w-[70px] font-mono text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {[3, 5, 10].map((s) => (
                  <SelectItem key={s} value={String(s)} className="font-mono text-xs">
                    {s}s
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Button
              size="sm"
              onClick={captureHidraw}
              className="gap-1.5"
              disabled={!selectedHidraw || hidrawLoading}
            >
              <IconPlayerPlay className="size-3.5" stroke={1.5} />
              Capture (password)
            </Button>

            <span className="text-[11px] text-muted-foreground">
              Captures only changes; safe bounded read.
            </span>
          </div>

          {hidrawError && (
            <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
              {hidrawError}
            </div>
          )}

          {hidrawCapture?.stderr && (
            <div className="mt-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 font-mono text-xs text-muted-foreground">
              stderr: {hidrawCapture.stderr}
            </div>
          )}

          <pre className={cn(
            "mt-3 max-h-[280px] overflow-auto rounded-lg border border-border/60 bg-black/30 dark:bg-black/40 p-3 font-mono text-[11px] leading-relaxed",
            (hidrawCapture?.samples?.length ?? 0) === 0 ? "text-muted-foreground" : "text-foreground"
          )}>
            {(hidrawCapture?.samples?.length ?? 0) === 0
              ? "No hidraw samples captured yet. Run capture and press Fn/F1/Fn+F1 during the window."
              : hidrawCapture!.samples
                  .map((s) => `${String(s.tsMs).padStart(5)}ms ${s.hex}`)
                  .join("\n")}
          </pre>
        </div>
      </div>
    </div>
  );
}
