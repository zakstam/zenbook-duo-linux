import { useState } from "react";
import type { Profile } from "@/types/duo";
import { activateProfile, deleteProfile } from "@/lib/tauri";
import { refreshProfiles, refreshStatus, useDispatch } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { IconPlayerPlay, IconTrash } from "@tabler/icons-react";

interface ProfileCardProps {
  profile: Profile;
}

export default function ProfileCard({ profile }: ProfileCardProps) {
  const dispatch = useDispatch();
  const [activating, setActivating] = useState(false);

  const handleActivate = async () => {
    setActivating(true);
    try {
      await activateProfile(profile.id);
      await refreshStatus(dispatch);
    } catch (err) {
      console.error("Failed to activate profile:", err);
    } finally {
      setActivating(false);
    }
  };

  const handleDelete = async () => {
    try {
      await deleteProfile(profile.id);
      await refreshProfiles(dispatch);
    } catch (err) {
      console.error("Failed to delete profile:", err);
    }
  };

  return (
    <div className="glass-card group rounded-xl p-5">
      <h4 className="mb-3 text-[14px] font-semibold tracking-tight">{profile.name}</h4>

      <div className="mb-4 space-y-1.5">
        <ProfileDetail label="Backlight" value={`${profile.backlightLevel}/3`} />
        <ProfileDetail label="Scale" value={`${profile.scale}x`} />
        <ProfileDetail label="Orientation" value={profile.orientation} capitalize />
        <ProfileDetail
          label="Dual Screen"
          value={profile.dualScreenEnabled ? "On" : "Off"}
          highlight={profile.dualScreenEnabled}
        />
      </div>

      <div className="flex gap-2">
        <Button
          size="sm"
          onClick={handleActivate}
          disabled={activating}
          className="flex-1 gap-1.5"
        >
          <IconPlayerPlay className="size-3.5" stroke={1.5} />
          {activating ? "Applying..." : "Activate"}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleDelete}
          className="text-muted-foreground hover:text-destructive"
        >
          <IconTrash className="size-3.5" stroke={1.5} />
        </Button>
      </div>
    </div>
  );
}

function ProfileDetail({
  label,
  value,
  capitalize,
  highlight,
}: {
  label: string;
  value: string;
  capitalize?: boolean;
  highlight?: boolean;
}) {
  return (
    <div className="flex items-center justify-between text-[12px]">
      <span className="text-muted-foreground">{label}</span>
      <span
        className={`font-mono font-medium ${capitalize ? "capitalize" : ""} ${
          highlight ? "text-primary" : "text-foreground"
        }`}
      >
        {value}
      </span>
    </div>
  );
}
