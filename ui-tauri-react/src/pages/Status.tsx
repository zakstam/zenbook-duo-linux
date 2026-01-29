import { useMemo } from "react";
import { useStore } from "@/lib/store";
import StatusCard from "@/components/StatusCard";
import { cn } from "@/lib/utils";
import {
  IconKeyboard,
  IconDeviceDesktop,
  IconWifi,
  IconServer,
} from "@tabler/icons-react";

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
        <StatusCard
          title="Keyboard"
          icon={<IconKeyboard className="size-4" stroke={1.5} />}
          className="animate-stagger-in stagger-1"
        >
          <div className="space-y-3">
            <StatusRow label="Connected">
              <StatusDot active={keyboardConnected} />
              <span className="font-mono text-xs">
                {keyboardConnected ? "Yes" : "No"}
              </span>
            </StatusRow>
            <StatusRow label="Connection">
              <span className="font-mono text-xs">
                {store.status.connectionType === "none"
                  ? "Disconnected"
                  : store.status.connectionType.toUpperCase()}
              </span>
            </StatusRow>
            <StatusRow label="Backlight">
              <div className="flex items-center gap-1.5">
                <div className="flex gap-0.5">
                  {[0, 1, 2, 3].map((level) => (
                    <div
                      key={level}
                      className={cn(
                        "h-2 w-3 rounded-sm transition-colors",
                        level <= store.status.backlightLevel
                          ? "bg-primary"
                          : "bg-muted"
                      )}
                    />
                  ))}
                </div>
                <span className="font-mono text-xs">
                  {store.status.backlightLevel}/3
                </span>
              </div>
            </StatusRow>
          </div>
        </StatusCard>

        <StatusCard
          title="Display"
          icon={<IconDeviceDesktop className="size-4" stroke={1.5} />}
          className="animate-stagger-in stagger-2"
        >
          <div className="space-y-3">
            <StatusRow label="Monitors">
              <span className="font-mono text-lg font-semibold leading-none text-foreground">
                {store.status.monitorCount}
              </span>
            </StatusRow>
            <StatusRow label="Brightness">
              <div className="flex items-center gap-2">
                <div className="h-1.5 w-16 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-300"
                    style={{ width: `${brightnessPercent}%` }}
                  />
                </div>
                <span className="font-mono text-xs">{brightnessPercent}%</span>
              </div>
            </StatusRow>
            <StatusRow label="Orientation">
              <span className="rounded-md bg-muted px-2 py-0.5 font-mono text-xs capitalize">
                {store.status.orientation}
              </span>
            </StatusRow>
          </div>
        </StatusCard>

        <StatusCard
          title="Connectivity"
          icon={<IconWifi className="size-4" stroke={1.5} />}
          className="animate-stagger-in stagger-3"
        >
          <div className="space-y-3">
            <StatusRow label="Wi-Fi">
              <StatusDot active={store.status.wifiEnabled} />
              <span className="font-mono text-xs">
                {store.status.wifiEnabled ? "Enabled" : "Disabled"}
              </span>
            </StatusRow>
            <StatusRow label="Bluetooth">
              <StatusDot active={store.status.bluetoothEnabled} />
              <span className="font-mono text-xs">
                {store.status.bluetoothEnabled ? "Enabled" : "Disabled"}
              </span>
            </StatusRow>
          </div>
        </StatusCard>

        <StatusCard
          title="Service"
          icon={<IconServer className="size-4" stroke={1.5} />}
          className="animate-stagger-in stagger-4"
        >
          <div className="space-y-3">
            <StatusRow label="zenbook-duo-user">
              <StatusDot
                active={store.status.serviceActive}
                error={!store.status.serviceActive}
              />
              <span className={cn(
                "font-mono text-xs font-medium",
                store.status.serviceActive ? "text-emerald-500" : "text-destructive"
              )}>
                {store.status.serviceActive ? "Active" : "Inactive"}
              </span>
            </StatusRow>
          </div>
        </StatusCard>
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
}: {
  active: boolean;
  error?: boolean;
}) {
  return (
    <span
      className={cn(
        "inline-block size-2 rounded-full",
        active
          ? "bg-emerald-500 glow-dot text-emerald-500 animate-pulse-glow"
          : error
            ? "bg-destructive glow-dot text-destructive"
            : "bg-muted-foreground/30"
      )}
    />
  );
}
