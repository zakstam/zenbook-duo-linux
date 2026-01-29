import { useState } from "react";
import {
  useStore,
  useDispatch,
  refreshProfiles,
} from "@/lib/store";
import { saveProfile } from "@/lib/tauri";
import ProfileCard from "@/components/ProfileCard";
import type { Profile } from "@/types/duo";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  IconPlus,
  IconRefresh,
  IconX,
  IconBookmark,
} from "@tabler/icons-react";

export default function Profiles() {
  const store = useStore();
  const dispatch = useDispatch();
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");

  const handleSaveCurrent = async () => {
    const name = newName.trim();
    if (!name) return;

    const profile: Profile = {
      id: name.toLowerCase().replace(/\s+/g, "-") + "-" + Date.now(),
      name,
      backlightLevel: store.status.backlightLevel,
      scale: store.settings.defaultScale,
      orientation: store.status.orientation,
      dualScreenEnabled: store.status.monitorCount > 1,
      displayLayout: null,
    };

    try {
      await saveProfile(profile);
      await refreshProfiles(dispatch);
      setNewName("");
      setShowCreate(false);
    } catch (err) {
      console.error("Failed to save profile:", err);
    }
  };

  return (
    <div>
      <div className="mb-6 flex items-start justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Profiles</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Save and restore hardware configurations
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => refreshProfiles(dispatch)}
            className="gap-1.5"
          >
            <IconRefresh className="size-3.5" stroke={1.5} />
            Refresh
          </Button>
          <Button
            size="sm"
            onClick={() => setShowCreate(!showCreate)}
            className="gap-1.5"
          >
            {showCreate ? (
              <>
                <IconX className="size-3.5" stroke={1.5} />
                Cancel
              </>
            ) : (
              <>
                <IconPlus className="size-3.5" stroke={1.5} />
                Save Current
              </>
            )}
          </Button>
        </div>
      </div>

      {showCreate && (
        <div className="glass-card mb-5 rounded-xl p-5 animate-page-enter">
          <h3 className="mb-4 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
            Save Current State
          </h3>
          <div className="flex items-center gap-3">
            <Input
              placeholder="Profile name..."
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSaveCurrent()}
              className="flex-1"
            />
            <Button onClick={handleSaveCurrent} disabled={!newName.trim()} className="gap-1.5">
              <IconBookmark className="size-3.5" stroke={1.5} />
              Save
            </Button>
          </div>
        </div>
      )}

      {store.profiles.length === 0 ? (
        <div className="glass-card flex flex-col items-center justify-center rounded-xl py-16 text-center">
          <div className="mb-3 flex size-12 items-center justify-center rounded-xl bg-muted">
            <IconBookmark className="size-5 text-muted-foreground" stroke={1.5} />
          </div>
          <p className="text-sm font-medium text-muted-foreground">No profiles saved yet</p>
          <p className="mt-1 text-[12px] text-muted-foreground/70">
            Save your current hardware state as a profile to quickly switch configurations
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {store.profiles.map((profile, i) => (
            <div key={profile.id} className={`animate-stagger-in stagger-${Math.min(i + 1, 6)}`}>
              <ProfileCard profile={profile} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
