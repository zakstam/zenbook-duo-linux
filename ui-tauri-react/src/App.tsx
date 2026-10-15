import { useEffect, useState } from "react";
import { useStoreInit, useStoreSelector } from "@/lib/store";
import Sidebar from "@/components/Sidebar";
import TitleBar from "@/components/TitleBar";
import Status from "@/pages/Status";
import Controls from "@/pages/Controls";
import Settings from "@/pages/Settings";
import Logs from "@/pages/Logs";
import DisplayLayout from "@/pages/DisplayLayout";
import Profiles from "@/features/profiles/ProfilesPage";
import EventMonitor from "@/pages/EventMonitor";
import Diagnostics from "@/pages/Diagnostics";
import Setup from "@/pages/Setup";
import { useTheme } from "next-themes";
import {
  onNativeSystemThemeChanged,
  resolveThemeForPreference,
} from "@/lib/theme";

export type Page =
  | "status"
  | "controls"
  | "settings"
  | "logs"
  | "display"
  | "profiles"
  | "events"
  | "diagnostics";

const pageComponents: Record<Page, React.ComponentType> = {
  status: Status,
  controls: Controls,
  settings: Settings,
  logs: Logs,
  display: DisplayLayout,
  profiles: Profiles,
  events: EventMonitor,
  diagnostics: Diagnostics,
};

export default function App() {
  const [currentPage, setCurrentPage] = useState<Page>("status");

  useStoreInit();
  const loading = useStoreSelector((s) => s.loading);
  const themePreference = useStoreSelector((s) => s.settings.theme);
  const setupCompleted = useStoreSelector((s) => s.settings.setupCompleted);
  const { setTheme } = useTheme();

  useEffect(() => {
    if (loading) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    async function applyThemePreference() {
      const theme = await resolveThemeForPreference(themePreference);
      if (!cancelled) {
        setTheme(theme);
      }
    }

    void applyThemePreference();

    if (themePreference === "system") {
      void onNativeSystemThemeChanged((theme) => {
        if (!cancelled) {
          setTheme(theme);
        }
      }).then((cleanup) => {
        if (cancelled) {
          cleanup();
        } else {
          unlisten = cleanup;
        }
      });
    }

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setTheme, loading, themePreference]);

  if (!loading && !setupCompleted) {
    return <Setup />;
  }

  const PageComponent = pageComponents[currentPage];

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background">
      <TitleBar />
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <Sidebar currentPage={currentPage} onNavigate={setCurrentPage} />
        <div className="flex flex-1 flex-col overflow-hidden">
          {/* Subtle top border accent */}
          <div className="h-px w-full bg-gradient-to-r from-transparent via-primary/20 to-transparent" />
          <main className="flex-1 overflow-y-auto px-8 py-7" key={currentPage}>
            <div className="mx-auto max-w-4xl">
              <PageComponent />
            </div>
          </main>
        </div>
      </div>
    </div>
  );
}
