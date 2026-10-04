import { useState } from "react";
import { toast } from "sonner";
import type { Orientation, Profile } from "@/types/duo";
import { profilesApi } from "@/lib/tauri-adapters";
import { refreshProfiles, useDispatch } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const SCALES = [1, 1.25, 1.5, 1.66, 1.75, 2];
const ORIENTATIONS: { id: Orientation; label: string }[] = [
  { id: "normal", label: "Normal" },
  { id: "left", label: "Left" },
  { id: "right", label: "Right" },
  { id: "inverted", label: "Upside down" },
];

interface ProfileEditorProps {
  profile: Profile;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Edits every setting a profile applies; the dialog remounts its form on each open. */
export default function ProfileEditor({ profile, open, onOpenChange }: ProfileEditorProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Edit profile</DialogTitle>
        </DialogHeader>
        {open && <ProfileForm profile={profile} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function ProfileForm({ profile, onDone }: { profile: Profile; onDone: () => void }) {
  const dispatch = useDispatch();
  const [draft, setDraft] = useState<Profile>({
    ...profile,
    bottomScale: profile.bottomScale ?? profile.scale,
    bottomOrientation: profile.bottomOrientation ?? profile.orientation,
  });
  const [saving, setSaving] = useState(false);
  const update = (patch: Partial<Profile>) => setDraft((current) => ({ ...current, ...patch }));

  const handleSave = async () => {
    const name = draft.name.trim();
    if (!name) return;
    setSaving(true);
    try {
      await profilesApi.saveProfile({ ...draft, name });
      await refreshProfiles(dispatch);
      onDone();
    } catch (err) {
      console.error("Failed to save profile:", err);
      toast.error(`Could not save profile: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <div className="grid gap-4">
        <Field label="Name">
          <Input value={draft.name} onChange={(e) => update({ name: e.target.value })} className="w-48" />
        </Field>
        <Field label="Keyboard backlight">
          <Select
            value={String(draft.backlightLevel)}
            onValueChange={(v) => update({ backlightLevel: parseInt(v) })}
          >
            <SelectTrigger className="w-48">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {[0, 1, 2, 3].map((level) => (
                <SelectItem key={level} value={String(level)}>
                  {level === 0 ? "Off" : `${level}/3`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <SectionTitle>Top screen</SectionTitle>
        <Field label="Scale">
          <ScaleSelect value={draft.scale} onChange={(scale) => update({ scale })} />
        </Field>
        <Field label="Rotation">
          <OrientationSelect value={draft.orientation} onChange={(orientation) => update({ orientation })} />
        </Field>

        <SectionTitle>Bottom screen</SectionTitle>
        <Field label="Turned on">
          <Switch
            checked={draft.dualScreenEnabled}
            onCheckedChange={(dualScreenEnabled) => update({ dualScreenEnabled })}
          />
        </Field>
        {draft.dualScreenEnabled && (
          <>
            <Field label="Scale">
              <ScaleSelect
                value={draft.bottomScale ?? draft.scale}
                onChange={(bottomScale) => update({ bottomScale })}
              />
            </Field>
            <Field label="Rotation">
              <OrientationSelect
                value={draft.bottomOrientation ?? draft.orientation}
                onChange={(bottomOrientation) => update({ bottomOrientation })}
              />
            </Field>
          </>
        )}
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button onClick={handleSave} disabled={saving || !draft.name.trim()}>
          {saving ? "Saving..." : "Save"}
        </Button>
      </DialogFooter>
    </>
  );
}

function ScaleSelect({ value, onChange }: { value: number; onChange: (scale: number) => void }) {
  // A saved scale the compositor snapped (e.g. 1.7476) still shows as itself.
  const scales = SCALES.includes(value) ? SCALES : [...SCALES, value].sort((a, b) => a - b);
  return (
    <Select value={String(value)} onValueChange={(v) => onChange(parseFloat(v))}>
      <SelectTrigger className="w-48">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {scales.map((scale) => (
          <SelectItem key={scale} value={String(scale)}>
            {`${scale}x (${Math.round(scale * 100)}%)`}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function OrientationSelect({
  value,
  onChange,
}: {
  value: Orientation;
  onChange: (orientation: Orientation) => void;
}) {
  return (
    <Select value={value} onValueChange={(v) => onChange(v as Orientation)}>
      <SelectTrigger className="w-48">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {ORIENTATIONS.map((o) => (
          <SelectItem key={o.id} value={o.id}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h4 className="-mb-1 mt-1 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
      {children}
    </h4>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <Label className="text-[13px]">{label}</Label>
      {children}
    </div>
  );
}
