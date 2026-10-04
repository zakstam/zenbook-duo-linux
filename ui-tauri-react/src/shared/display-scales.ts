/** Display scale presets offered by the scale dropdowns (the installer's suggestions plus common steps). */
export const DISPLAY_SCALE_PRESETS = [1, 1.25, 1.5, 1.66, 1.75, 2] as const;

/**
 * The presets plus `current` when it is not one of them (the installer accepts any value),
 * so a dropdown never shows a blank selection for a saved scale.
 */
export function displayScaleOptions(current?: number): number[] {
  const options: number[] = [...DISPLAY_SCALE_PRESETS];
  if (current !== undefined && Number.isFinite(current) && !options.includes(current)) {
    options.push(current);
    options.sort((a, b) => a - b);
  }
  return options;
}

/** Label such as "1.0x" or "1.75x"; whole numbers keep one decimal. */
export function formatDisplayScale(scale: number): string {
  return `${Number.isInteger(scale) ? scale.toFixed(1) : scale}x`;
}
