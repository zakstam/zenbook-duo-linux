/**
 * Display scale presets offered by the scale dropdowns and the installer.
 *
 * Each is a multiple of 1/120 (the Wayland fractional-scale step) that divides the
 * Duo's 2880x1800 panels into whole logical pixels, and the only such scales GNOME
 * offers there, so every desktop applies them unchanged. Off-grid values like 1.66
 * or 1.75 get replaced by each desktop's own nearest scale and the screens drift.
 * Keep in step with `src-tauri/src/features/display_layout/scale.rs`.
 */
export const DISPLAY_SCALE_PRESETS = [1, 1.25, 4 / 3, 1.5, 5 / 3, 2] as const;

/** A scale this close to a preset is that preset (5/3 reported as 1.6666666, niri's 1.6583, the installer's 1.67). */
const PRESET_TOLERANCE = 0.01;

/** Scales earlier versions offered, and the preset each now shows as. */
const RETIRED_SCALES: ReadonlyArray<readonly [number, number]> = [[1.75, 5 / 3]];

/** The preset `scale` stands for, or `scale` itself when it is none. */
export function canonicalDisplayScale(scale: number): number {
  if (!Number.isFinite(scale)) return scale;
  const retired = RETIRED_SCALES.find(([old]) => Math.abs(scale - old) < PRESET_TOLERANCE);
  if (retired) return retired[1];
  return DISPLAY_SCALE_PRESETS.find((preset) => Math.abs(scale - preset) < PRESET_TOLERANCE) ?? scale;
}

/**
 * The presets plus `current` when it stands for none of them (the installer accepts any
 * value), so a dropdown never shows a blank selection. Select the option with
 * `canonicalDisplayScale(current)`.
 */
export function displayScaleOptions(current?: number): number[] {
  const options: number[] = [...DISPLAY_SCALE_PRESETS];
  if (current === undefined || !Number.isFinite(current)) return options;
  const selected = canonicalDisplayScale(current);
  if (!options.includes(selected)) {
    options.push(selected);
    options.sort((a, b) => a - b);
  }
  return options;
}

/** Label such as "1.0x", "1.25x" or "1.67x": at most two decimals, whole numbers keep one. */
export function formatDisplayScale(scale: number): string {
  if (Number.isInteger(scale)) return `${scale.toFixed(1)}x`;
  return `${Number(scale.toFixed(2))}x`;
}
