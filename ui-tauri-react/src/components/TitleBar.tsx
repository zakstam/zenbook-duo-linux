import {
  IconMinus,
  IconSquare,
  IconX,
} from "@tabler/icons-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { cn } from "@/lib/utils";
import { useStore } from "@/lib/store";

type TitleBarButtonProps = {
  label: string;
  variant?: "neutral" | "warm" | "cool" | "danger";
  onClick: () => void;
  children: React.ReactNode;
};

const appWindow = getCurrentWindow();

const VARIANT_CLASSES: Record<NonNullable<TitleBarButtonProps["variant"]>, string> = {
  neutral: "hover:bg-muted/80 hover:text-foreground focus-visible:bg-muted/80",
  // amber — minimize. Slightly warm to read as "stash it".
  warm:
    "hover:bg-[oklch(0.92_0.08_75)] hover:text-[oklch(0.30_0.10_60)] focus-visible:bg-[oklch(0.92_0.08_75)] dark:hover:bg-[oklch(0.32_0.10_70)] dark:hover:text-[oklch(0.90_0.08_75)]",
  // teal — maximize. Cool, matches primary accent.
  cool:
    "hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent",
  // red — close.
  danger:
    "hover:bg-destructive hover:text-destructive-foreground focus-visible:bg-destructive focus-visible:text-destructive-foreground",
};

const VARIANT_FILAMENT: Record<NonNullable<TitleBarButtonProps["variant"]>, string> = {
  neutral: "bg-foreground/50",
  warm: "bg-[oklch(0.75_0.18_70)] shadow-[0_0_6px_oklch(0.75_0.18_70)]",
  cool: "bg-primary shadow-[0_0_6px_currentColor] text-primary",
  danger: "bg-destructive shadow-[0_0_8px_currentColor] text-destructive",
};

function TitleBarButton({
  label,
  variant = "neutral",
  onClick,
  children,
}: TitleBarButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        "group relative inline-flex h-9 w-11 items-center justify-center text-foreground/65 transition-colors duration-150",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring/40 focus-visible:ring-inset",
        VARIANT_CLASSES[variant],
      )}
    >
      <span className="transition-transform duration-200 group-hover:scale-110 group-active:scale-95">
        {children}
      </span>
      {/* LED filament — lights up on hover */}
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-x-3 bottom-0 h-px rounded-full opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-visible:opacity-100",
          VARIANT_FILAMENT[variant],
        )}
      />
    </button>
  );
}

export default function TitleBar() {
  const { versionInfo } = useStore();
  const linkUp = versionInfo.serviceAvailable;

  return (
    <header
      className={cn(
        "relative flex h-9 shrink-0 select-none items-center",
        "border-b border-border/60",
        "bg-gradient-to-b from-background to-background/70",
      )}
    >
      {/* Hairline accent under the info strip — fades at edges */}
      <span
        aria-hidden
        className="pointer-events-none absolute inset-x-16 bottom-0 h-px bg-gradient-to-r from-transparent via-primary/20 to-transparent"
      />

      {/* Left: tiny callsign — pointer-events-none so drag passes through */}
      <span
        aria-hidden
        data-tauri-drag-region
        className="pointer-events-none flex items-center gap-2 pl-3.5 font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground/55"
      >
        <span className="size-1 rounded-[1px] bg-muted-foreground/45" />
        Zenbook<span className="text-muted-foreground/30">·</span>Duo
      </span>

      {/* Flexible drag region */}
      <div data-tauri-drag-region className="h-full flex-1" />

      {/* Right: live daemon link indicator */}
      <div
        aria-hidden
        data-tauri-drag-region
        className="pointer-events-none flex items-center gap-1.5 pr-4 font-mono text-[10px] uppercase tracking-[0.16em]"
        title={linkUp ? "Daemon connected" : "Daemon offline"}
      >
        <span
          className={cn(
            "size-1.5 rounded-full transition-colors duration-300",
            linkUp
              ? "bg-primary text-primary shadow-[0_0_6px_currentColor] animate-pulse-glow"
              : "bg-muted-foreground/40",
          )}
        />
        <span
          className={cn(
            "transition-colors duration-300",
            linkUp ? "text-foreground/75" : "text-muted-foreground/55",
          )}
        >
          {linkUp ? "Link" : "Offline"}
        </span>
      </div>

      {/* Window controls */}
      <nav className="flex h-full items-center" aria-label="Window controls">
        <TitleBarButton
          label="Minimize window"
          variant="warm"
          onClick={() => void appWindow.minimize()}
        >
          <IconMinus className="size-3.5" stroke={1.75} />
        </TitleBarButton>
        <TitleBarButton
          label="Maximize or restore window"
          variant="cool"
          onClick={() => void appWindow.toggleMaximize()}
        >
          <IconSquare className="size-3" stroke={1.75} />
        </TitleBarButton>
        <TitleBarButton
          label="Close window to tray"
          variant="danger"
          onClick={() => void appWindow.close()}
        >
          <IconX className="size-3.5" stroke={1.75} />
        </TitleBarButton>
      </nav>
    </header>
  );
}
