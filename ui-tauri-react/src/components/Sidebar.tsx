import type { Page } from "@/App";
import { useEffect, useState } from "react";
import ThemeToggle from "@/components/ThemeToggle";
import { cn } from "@/lib/utils";
import {
  IconCircleDot,
  IconAdjustments,
  IconSettings,
  IconFileText,
  IconLayout,
  IconUsers,
  IconBolt,
  IconBug,
} from "@tabler/icons-react";

const navItems: { id: Page; label: string; icon: React.ComponentType<{ className?: string; stroke?: number }> }[] = [
  { id: "status", label: "Status", icon: IconCircleDot },
  { id: "controls", label: "Controls", icon: IconAdjustments },
  { id: "profiles", label: "Profiles", icon: IconUsers },
  { id: "settings", label: "Settings", icon: IconSettings },
  { id: "logs", label: "Logs", icon: IconFileText },
  { id: "events", label: "Events", icon: IconBolt },
  { id: "diagnostics", label: "Diagnostics", icon: IconBug },
];

interface SidebarProps {
  currentPage: Page;
  onNavigate: (page: Page) => void;
}

export default function Sidebar({ currentPage, onNavigate }: SidebarProps) {
  const [version, setVersion] = useState<string>("v0.2.0");

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { getVersion } = await import("@tauri-apps/api/app");
        const v = await getVersion();
        if (!cancelled && v) setVersion(`v${v}`);
      } catch {
        // Non-Tauri context or restricted API; keep fallback.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <nav className="flex h-full w-[220px] shrink-0 flex-col bg-sidebar">
      {/* Brand */}
      <div className="px-5 pt-6 pb-5">
        <div className="flex items-center gap-2.5">
          <div className="flex size-8 items-center justify-center rounded-lg bg-primary/10">
            <IconLayout className="size-4 text-primary" stroke={1.75} />
          </div>
          <div>
            <h2 className="text-[13px] font-semibold tracking-tight text-foreground">
              Zenbook Duo
            </h2>
            <span className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
              Control Panel
            </span>
          </div>
        </div>
      </div>

      {/* Separator */}
      <div className="mx-4 h-px bg-border/60" />

      {/* Navigation */}
      <div className="flex flex-1 flex-col gap-0.5 px-3 pt-4">
        <span className="mb-1.5 px-2 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          Navigation
        </span>
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentPage === item.id;
          return (
            <button
              key={item.id}
              className={cn(
                "group relative flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] font-medium transition-all duration-150",
                isActive
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"
              )}
              onClick={() => onNavigate(item.id)}
            >
              {/* Active indicator bar */}
              {isActive && (
                <div className="absolute left-0 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-r-full bg-primary" />
              )}
              <Icon
                className={cn(
                  "size-[18px] transition-colors",
                  isActive ? "text-primary" : "text-muted-foreground group-hover:text-foreground"
                )}
                stroke={isActive ? 2 : 1.5}
              />
              {item.label}
            </button>
          );
        })}
      </div>

      {/* Footer */}
      <div className="mx-4 h-px bg-border/60" />
      <div className="px-3 py-3">
        <ThemeToggle />
      </div>
      <div className="px-5 pb-4">
        <span className="font-mono text-[10px] text-muted-foreground/50">
          {version}
        </span>
      </div>
    </nav>
  );
}
