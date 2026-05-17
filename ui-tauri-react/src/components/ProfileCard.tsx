import { useState } from "react";
import type { Profile } from "@/types/duo";
import { profilesApi } from "@/lib/tauri-adapters";
import { refreshProfiles, refreshStatus, useDispatch } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  IconPlayerPlay,
  IconTrash,
  IconSun,
  IconArrowsMaximize,
  IconRotate,
  IconScreenShare,
} from "@tabler/icons-react";

interface ProfileCardProps {
  profile: Profile;
}

export default function ProfileCard({ profile }: ProfileCardProps) {
  const dispatch = useDispatch();
  const [activating, setActivating] = useState(false);

  const handleActivate = async () => {
    setActivating(true);
    try {
      await profilesApi.activateProfile(profile.id);
      await refreshStatus(dispatch);
    } catch (err) {
      console.error("Failed to activate profile:", err);
    } finally {
      setActivating(false);
    }
  };

  const handleDelete = async () => {
    try {
      await profilesApi.deleteProfile(profile.id);
      await refreshProfiles(dispatch);
    } catch (err) {
      console.error("Failed to delete profile:", err);
    }
  };

  return (
    <div className="glass-card group rounded-xl p-5 transition-shadow hover:shadow-md hover:shadow-black/5">
      <div className="mb-4 flex items-start justify-between">
        <h4 className="text-[14px] font-semibold tracking-tight">{profile.name}</h4>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleDelete}
          className="size-7 p-0 text-muted-foreground/50 opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100"
        >
          <IconTrash className="size-3.5" stroke={1.5} />
        </Button>
      </div>

      <div className="mb-4 grid grid-cols-2 gap-2">
        <ProfileDetail icon={IconSun} label="Backlight" value={`${profile.backlightLevel}/3`} />
        <ProfileDetail icon={IconArrowsMaximize} label="Scale" value={`${profile.scale}x`} />
        <ProfileDetail icon={IconRotate} label="Orientation" value={profile.orientation} capitalize />
        <ProfileDetail
          icon={IconScreenShare}
          label="Dual Screen"
          value={profile.dualScreenEnabled ? "On" : "Off"}
          highlight={profile.dualScreenEnabled}
        />
      </div>

      <Button
        size="sm"
        onClick={handleActivate}
        disabled={activating}
        className="w-full gap-1.5"
      >
        <IconPlayerPlay className="size-3.5" stroke={1.5} />
        {activating ? "Applying..." : "Activate"}
      </Button>
    </div>
  );
}

function ProfileDetail({
  icon: Icon,
  label,
  value,
  capitalize,
  highlight,
}: {
  icon: React.ComponentType<{ className?: string; stroke?: number }>;
  label: string;
  value: string;
  capitalize?: boolean;
  highlight?: boolean;
}) {
  return (
    <div className="rounded-lg bg-muted/40 px-2.5 py-2">
      <div className="mb-1 flex items-center gap-1.5">
        <Icon className="size-3 text-muted-foreground/60" stroke={1.5} />
        <span className="text-[10px] text-muted-foreground">{label}</span>
      </div>
      <span
        className={cn(
          "font-mono text-[12px] font-semibold",
          capitalize && "capitalize",
          highlight ? "text-primary" : "text-foreground"
        )}
      >
        {value}
      </span>
    </div>
  );
}
