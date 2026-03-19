import { useMemo } from "react";
import { useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import {
  IconKeyboard,
  IconDeviceDesktop,
  IconWifi,
  IconServer,
  IconBluetooth,
  IconPlugConnected,
} from "@tabler/icons-react";

const cardAccents = {
  keyboard: { icon: "bg-teal-500/12 text-teal-500 dark:bg-teal-400/10 dark:text-teal-400", border: "border-l-teal-500/40" },
  display: { icon: "bg-blue-500/12 text-blue-500 dark:bg-blue-400/10 dark:text-blue-400", border: "border-l-blue-500/40" },
  connectivity: { icon: "bg-violet-500/12 text-violet-500 dark:bg-violet-400/10 dark:text-violet-400", border: "border-l-violet-500/40" },
  service: { icon: "bg-emerald-500/12 text-emerald-500 dark:bg-emerald-400/10 dark:text-emerald-400", border: "border-l-emerald-500/40" },
} as const;

export default function Status() {
  const store = useStore();

  const keyboardConnected = store.status.connectionType !== "none";

  const brightnessPercent = useMemo(
    () =>
      store.status.maxBrightness > 0
        ? Math.round(
            (store.status.displayBrightness / store.status.maxBrightness) * 100
          )
        : 0,
    [store.status.displayBrightness, store.status.maxBrightness]
  );

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">System Status</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Live overview of your Zenbook Duo hardware
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {/* Keyboard */}
        <div className={cn("glass-card rounded-xl border-l-[3px] p-5 animate-stagger-in stagger-1", cardAccents.keyboard.border)}>
          <div className="mb-4 flex items-center gap-2.5">
            <div className={cn("flex size-7 items-center justify-center rounded-lg", cardAccents.keyboard.icon)}>
              <IconKeyboard className="size-3.5" stroke={1.75} />
            </div>
            <h3 className="text-[13px] font-semibold text-foreground">Keyboard</h3>
            <StatusDot active={keyboardConnected} className="ml-auto" />
          </div>
          <div className="space-y-3">
            <StatusRow label="Connected">
              <span className={cn("font-mono text-xs font-medium", keyboardConnected ? "text-emerald-500" : "text-muted-foreground")}>
                {keyboardConnected ? "Yes" : "No"}
              </span>
            </StatusRow>
            <StatusRow label="Connection">
              <div className="flex items-center gap-1.5">
                {store.status.connectionType === "bluetooth" && <IconBluetooth className="size-3 text-blue-500" stroke={1.5} />}
                {store.status.connectionType === "usb" && <IconPlugConnected className="size-3 text-teal-500" stroke={1.5} />}
                <span className="font-mono text-xs">
                  {store.status.connectionType === "none"
                    ? "Disconnected"
                    : store.status.connectionType.toUpperCase()}
                </span>
              </div>
            </StatusRow>
            <StatusRow label="Backlight">
              <div className="flex items-center gap-2">
                <div className="flex gap-[3px]">
                  {[0, 1, 2, 3].map((level) => (
                    <div
                      key={level}
                      className={cn(
                        "h-2.5 w-[14px] rounded-[3px] transition-all duration-300",
                        level <= store.status.backlightLevel
                          ? "bg-teal-500 shadow-sm shadow-teal-500/30"
                          : "bg-muted"
                      )}
                    />
                  ))}
                </div>
                <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
                  {store.status.backlightLevel}/3
                </span>
              </div>
            </StatusRow>
          </div>
        </div>

        {/* Display */}
        <div className={cn("glass-card rounded-xl border-l-[3px] p-5 animate-stagger-in stagger-2", cardAccents.display.border)}>
          <div className="mb-4 flex items-center gap-2.5">
            <div className={cn("flex size-7 items-center justify-center rounded-lg", cardAccents.display.icon)}>
              <IconDeviceDesktop className="size-3.5" stroke={1.75} />
            </div>
            <h3 className="text-[13px] font-semibold text-foreground">Display</h3>
            <span className="ml-auto rounded-md bg-blue-500/10 px-2 py-0.5 font-mono text-[11px] font-semibold tabular-nums text-blue-500">
              {store.status.monitorCount}
            </span>
          </div>
          <div className="space-y-3">
            <StatusRow label="Brightness">
              <div className="flex items-center gap-2.5">
                <div className="relative h-2 w-20 overflow-hidden rounded-full bg-muted">
                  <div
                    className="absolute inset-y-0 left-0 rounded-full bg-gradient-to-r from-blue-500/80 to-blue-400 transition-all duration-500 ease-out"
                    style={{ width: `${brightnessPercent}%` }}
                  />
                </div>
                <span className="min-w-[2.5rem] text-right font-mono text-[11px] tabular-nums text-muted-foreground">
                  {brightnessPercent}%
                </span>
              </div>
            </StatusRow>
            <StatusRow label="Orientation">
              <span className="rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-medium capitalize">
                {store.status.orientation}
              </span>
            </StatusRow>
          </div>
        </div>

        {/* Connectivity */}
        <div className={cn("glass-card rounded-xl border-l-[3px] p-5 animate-stagger-in stagger-3", cardAccents.connectivity.border)}>
          <div className="mb-4 flex items-center gap-2.5">
            <div className={cn("flex size-7 items-center justify-center rounded-lg", cardAccents.connectivity.icon)}>
              <IconWifi className="size-3.5" stroke={1.75} />
            </div>
            <h3 className="text-[13px] font-semibold text-foreground">Connectivity</h3>
          </div>
          <div className="space-y-3">
            <StatusRow label="Wi-Fi">
              <div className="flex items-center gap-2">
                <StatusDot active={store.status.wifiEnabled} />
                <span className={cn(
                  "font-mono text-xs font-medium",
                  store.status.wifiEnabled ? "text-emerald-500" : "text-muted-foreground"
                )}>
                  {store.status.wifiEnabled ? "Enabled" : "Disabled"}
                </span>
              </div>
            </StatusRow>
            <StatusRow label="Bluetooth">
              <div className="flex items-center gap-2">
                <StatusDot active={store.status.bluetoothEnabled} />
                <span className={cn(
                  "font-mono text-xs font-medium",
                  store.status.bluetoothEnabled ? "text-emerald-500" : "text-muted-foreground"
                )}>
                  {store.status.bluetoothEnabled ? "Enabled" : "Disabled"}
                </span>
              </div>
            </StatusRow>
          </div>
        </div>

        {/* Service */}
        <div className={cn("glass-card rounded-xl border-l-[3px] p-5 animate-stagger-in stagger-4", cardAccents.service.border)}>
          <div className="mb-4 flex items-center gap-2.5">
            <div className={cn("flex size-7 items-center justify-center rounded-lg", cardAccents.service.icon)}>
              <IconServer className="size-3.5" stroke={1.75} />
            </div>
            <h3 className="text-[13px] font-semibold text-foreground">Service</h3>
          </div>
          <div className="space-y-3">
            <StatusRow label="Rust runtime">
              <div className="flex items-center gap-2">
                <StatusDot
                  active={store.status.serviceActive}
                  error={!store.status.serviceActive}
                />
                <span className={cn(
                  "rounded-md px-2 py-0.5 font-mono text-[11px] font-semibold",
                  store.status.serviceActive
                    ? "bg-emerald-500/10 text-emerald-500"
                    : "bg-destructive/10 text-destructive"
                )}>
                  {store.status.serviceActive ? "Active" : "Inactive"}
                </span>
              </div>
            </StatusRow>
          </div>
        </div>
      </div>
    </div>
  );
}

function StatusRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[13px] text-muted-foreground">{label}</span>
      <span className="flex items-center gap-1.5">{children}</span>
    </div>
  );
}

function StatusDot({
  active,
  error,
  className,
}: {
  active: boolean;
  error?: boolean;
  className?: string;
}) {
  return (
    <span className={cn("relative inline-flex", className)}>
      {active && (
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-40" />
      )}
      <span
        className={cn(
          "relative inline-block size-2 rounded-full",
          active
            ? "bg-emerald-500"
            : error
              ? "bg-destructive"
              : "bg-muted-foreground/30"
        )}
      />
    </span>
  );
}
