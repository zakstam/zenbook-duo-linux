import { useState } from "react";
import BacklightSlider from "@/components/BacklightSlider";
import OrientationButtons from "@/components/OrientationButtons";
import { restartService } from "@/lib/tauri";
import { refreshStatus, useDispatch } from "@/lib/store";
import { Button } from "@/components/ui/button";
import {
  IconRefresh,
  IconKeyboard,
  IconRotate,
  IconServer,
} from "@tabler/icons-react";

export default function Controls() {
  const dispatch = useDispatch();
  const [restarting, setRestarting] = useState(false);

  const handleRestart = async () => {
    setRestarting(true);
    try {
      await restartService();
      setTimeout(async () => {
        await refreshStatus(dispatch);
        setRestarting(false);
      }, 2000);
    } catch (err) {
      console.error("Failed to restart service:", err);
      setRestarting(false);
    }
  };

  return (
    <div>
      <div className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">Controls</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Adjust hardware settings in real time
        </p>
      </div>

      <div className="space-y-5">
        <div className="glass-card animate-stagger-in stagger-1 rounded-xl p-5">
          <div className="mb-4 flex items-center gap-2">
            <IconKeyboard className="size-4 text-primary/80" stroke={1.5} />
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              Keyboard Backlight
            </h3>
          </div>
          <BacklightSlider />
        </div>

        <div className="glass-card animate-stagger-in stagger-2 rounded-xl p-5">
          <div className="mb-4 flex items-center gap-2">
            <IconRotate className="size-4 text-primary/80" stroke={1.5} />
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              Screen Orientation
            </h3>
          </div>
          <OrientationButtons />
        </div>

        <div className="glass-card animate-stagger-in stagger-3 rounded-xl p-5">
          <div className="mb-4 flex items-center gap-2">
            <IconServer className="size-4 text-primary/80" stroke={1.5} />
            <h3 className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
              Service Control
            </h3>
          </div>
          <p className="mb-4 text-[13px] text-muted-foreground">
            Restart the zenbook-duo-user background service if experiencing issues.
          </p>
          <Button
            variant="outline"
            onClick={handleRestart}
            disabled={restarting}
            className="gap-2"
          >
            <IconRefresh className={`size-4 ${restarting ? "animate-spin" : ""}`} stroke={1.5} />
            {restarting ? "Restarting..." : "Restart Service"}
          </Button>
        </div>
      </div>
    </div>
  );
}
