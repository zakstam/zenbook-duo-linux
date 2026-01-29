import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
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
      if (!devices.some((d) => d.eventPath === selectedEvdev) && devices.length > 0) {
        setSelectedEvdev(devices[0].eventPath);
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
      if (!devices.some((d) => d.id === selectedHidId)) setSelectedHidId(devices[0]?.id ?? "");

      const nextHidraw = Array.from(
        new Set(devices.flatMap((d) => d.hidrawNodes))
      ).sort();
      if (nextHidraw.length > 0 && !nextHidraw.includes(selectedHidraw)) {
        setSelectedHidraw(nextHidraw[0]);
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
        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-1">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              Evdev Capture
            </h3>
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
              <select
                className="h-8 rounded-md border border-border bg-background px-2 font-mono text-xs"
                value={selectedEvdev}
                onChange={(e) => setSelectedEvdev(e.target.value)}
              >
                {evdev.map((d) => (
                  <option key={d.eventPath} value={d.eventPath}>
                    {d.eventPath} - {d.name}
                  </option>
                ))}
              </select>

              <select
                className="h-8 rounded-md border border-border bg-background px-2 font-mono text-xs"
                value={String(evdevSeconds)}
                onChange={(e) => setEvdevSeconds(Number(e.target.value))}
              >
                {[3, 5, 10, 15].map((s) => (
                  <option key={s} value={s}>
                    {s}s
                  </option>
                ))}
              </select>

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

              {evdevSelected && (
                <span className="text-xs text-muted-foreground">
                  <span className="font-mono">phys</span> {evdevSelected.phys ?? "-"} ·{" "}
                  <span className="font-mono">id</span>{" "}
                  {(evdevSelected.vendor ?? "??") + ":" + (evdevSelected.product ?? "??")}
                </span>
              )}
            </div>

            {evdevError && (
              <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
                {evdevError}
              </div>
            )}

            <pre className={cn(
              "max-h-[280px] overflow-auto rounded-md border border-border bg-black/40 p-3 font-mono text-[11px] leading-relaxed",
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

        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-2">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              HID Topology
            </h3>
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
            <span className="font-mono text-xs text-muted-foreground">vid</span>
            <input
              className="h-8 w-[90px] rounded-md border border-border bg-background px-2 font-mono text-xs"
              value={vid}
              onChange={(e) => setVid(e.target.value)}
            />
            <span className="font-mono text-xs text-muted-foreground">pid</span>
            <input
              className="h-8 w-[90px] rounded-md border border-border bg-background px-2 font-mono text-xs"
              value={pid}
              onChange={(e) => setPid(e.target.value)}
            />
            <Button size="sm" onClick={refreshHid} className="gap-1.5">
              <IconRefresh className="size-3.5" stroke={1.5} />
              Load
            </Button>
          </div>

          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
            <div className="rounded-md border border-border bg-muted/20 p-3">
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                Devices
              </div>
              <select
                className="h-8 w-full rounded-md border border-border bg-background px-2 font-mono text-xs"
                value={selectedHidId}
                onChange={(e) => setSelectedHidId(e.target.value)}
              >
                {hid.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.id} ({d.driver ?? "?"})
                  </option>
                ))}
              </select>
              <div className="mt-2 font-mono text-[11px] text-muted-foreground">
                <div>hidName: {hidSelected?.hidName ?? "-"}</div>
                <div>hidPhys: {hidSelected?.hidPhys ?? "-"}</div>
                <div>hidId: {hidSelected?.hidId ?? "-"}</div>
              </div>
              <div className="mt-2 text-xs text-muted-foreground">
                HID "topology" = the keyboard exposes multiple HID interfaces. Each interface maps to
                different Linux devices (evdev + hidraw). Pick the interface that lists the evdev
                node you care about (like <span className="font-mono">/dev/input/event28</span>),
                then capture both evdev and its corresponding hidraw.
              </div>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={loadDescriptor}
                  disabled={!selectedHidId || descriptorLoading}
                >
                  Read report_descriptor
                </Button>
              </div>
            </div>

            <div className="rounded-md border border-border bg-muted/20 p-3">
              <div className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
                Mappings
              </div>
              <div className="space-y-2 font-mono text-[11px]">
                <div>
                  <div className="text-muted-foreground">hidraw</div>
                  <div className="text-foreground">
                    {(hidSelected?.hidrawNodes ?? []).join(" ") || "-"}
                  </div>
                </div>
                <div>
                  <div className="text-muted-foreground">evdev</div>
                  <div className="text-foreground">
                    {(hidSelected?.inputEventNodes ?? []).join(" ") || "-"}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {descriptorError && (
            <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
              {descriptorError}
            </div>
          )}

          {descriptor && (
            <div className="mt-3 rounded-md border border-border bg-black/40 p-3">
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
                >
                  Export
                </Button>
              </div>
              <pre className="max-h-[240px] overflow-auto font-mono text-[11px] text-foreground">
                {descriptor.hex}
              </pre>
            </div>
          )}
        </div>

        <div className="glass-card rounded-xl p-5 animate-stagger-in stagger-3">
          <div className="mb-3 flex items-center justify-between">
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              Hidraw Capture (pkexec)
            </h3>
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
            <select
              className="h-8 rounded-md border border-border bg-background px-2 font-mono text-xs"
              value={selectedHidraw}
              onChange={(e) => setSelectedHidraw(e.target.value)}
            >
              <option value="">Select hidraw</option>
              {hidrawNodes.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>

            <select
              className="h-8 rounded-md border border-border bg-background px-2 font-mono text-xs"
              value={String(hidrawSeconds)}
              onChange={(e) => setHidrawSeconds(Number(e.target.value))}
            >
              {[3, 5, 10].map((s) => (
                <option key={s} value={s}>
                  {s}s
                </option>
              ))}
            </select>

            <Button
              size="sm"
              onClick={captureHidraw}
              className="gap-1.5"
              disabled={!selectedHidraw || hidrawLoading}
            >
              <IconPlayerPlay className="size-3.5" stroke={1.5} />
              Capture (password)
            </Button>

            <span className="text-xs text-muted-foreground">
              Captures only changes; safe bounded read.
            </span>
          </div>

          {hidrawError && (
            <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 font-mono text-xs text-destructive">
              {hidrawError}
            </div>
          )}

          {hidrawCapture?.stderr && (
            <div className="mt-3 rounded-md border border-border bg-muted/20 px-3 py-2 font-mono text-xs text-muted-foreground">
              stderr: {hidrawCapture.stderr}
            </div>
          )}

          <pre className={cn(
            "mt-3 max-h-[280px] overflow-auto rounded-md border border-border bg-black/40 p-3 font-mono text-[11px] leading-relaxed",
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
