import type { DisplayInfo, DisplayLayout } from "@/types/duo";

export const DISPLAY_CANVAS_SCALE = 0.12;
export const TOP_DISPLAY_CONNECTOR = "eDP-1";
export const BOTTOM_DISPLAY_CONNECTOR = "eDP-2";

export interface DisplayCanvasNodeModel {
  id: string;
  position: { x: number; y: number };
  draggable: boolean;
  data: {
    connector: string;
    width: number;
    height: number;
    logicalHeight: number;
    refreshRate: number;
    primary: boolean;
    scale: number;
  };
  style: { width: number; height: number };
}

export function isDuoDisplay(connector: string) {
  return connector === TOP_DISPLAY_CONNECTOR || connector === BOTTOM_DISPLAY_CONNECTOR;
}

export function displayLogicalSize(display: DisplayInfo) {
  const rotated = display.transform === 90 || display.transform === 270;
  const physicalWidth = rotated ? display.height : display.width;
  const physicalHeight = rotated ? display.width : display.height;
  const scale = Math.max(display.scale, 0.1);

  return {
    width: physicalWidth / scale,
    height: physicalHeight / scale,
  };
}

function stackedLogicalHeight(display: DisplayInfo) {
  return Math.ceil(displayLogicalSize(display).height);
}

export function normalizeDuoDisplays(layout: DisplayLayout): DisplayLayout {
  const topDisplay = layout.displays.find((display) => display.connector === TOP_DISPLAY_CONNECTOR);
  if (!topDisplay) {
    return layout;
  }

  const topLogicalHeight = stackedLogicalHeight(topDisplay);

  return {
    displays: layout.displays.map((display) => {
      if (display.connector === TOP_DISPLAY_CONNECTOR) {
        return { ...display, x: 0, y: 0 };
      }

      if (display.connector === BOTTOM_DISPLAY_CONNECTOR) {
        return { ...display, x: 0, y: topLogicalHeight };
      }

      return display;
    }),
  };
}

export function snapDisplayPosition(pos: { x: number; y: number }) {
  return {
    x: Math.round(pos.x / DISPLAY_CANVAS_SCALE),
    y: Math.round(pos.y / DISPLAY_CANVAS_SCALE),
  };
}

export function layoutToCanvasNodes(layout: DisplayLayout): DisplayCanvasNodeModel[] {
  // Drawn where the compositor has each screen; only Apply stacks the Duo screens.
  const primary = layout.displays.find((d) => d.primary) ?? layout.displays[0];
  const baseX = primary?.x ?? 0;
  const baseY = primary?.y ?? 0;
  const primaryLogical = primary ? displayLogicalSize(primary) : null;
  const seen = new Map<string, number>();

  return layout.displays.map((display) => {
    const logical = displayLogicalSize(display);
    const key = `${display.x},${display.y}`;
    const count = seen.get(key) ?? 0;
    seen.set(key, count + 1);

    const staggerY = count === 0 ? 0 : Math.floor((primaryLogical?.height ?? logical.height) * count);
    const effectiveX = count === 0 ? display.x : baseX;
    const effectiveY = count === 0 ? display.y : baseY + staggerY;
    const scaledWidth = Math.round(logical.width * DISPLAY_CANVAS_SCALE);
    const scaledHeight = Math.round(logical.height * DISPLAY_CANVAS_SCALE);

    return {
      id: display.connector,
      position: { x: effectiveX * DISPLAY_CANVAS_SCALE, y: effectiveY * DISPLAY_CANVAS_SCALE },
      draggable: !isDuoDisplay(display.connector),
      data: {
        connector: display.connector,
        width: display.width,
        height: display.height,
        logicalHeight: logical.height,
        refreshRate: display.refreshRate,
        primary: display.primary,
        scale: display.scale,
      },
      // True to size, so screens that look adjacent are adjacent.
      style: { width: scaledWidth, height: scaledHeight },
    };
  });
}

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Share of the shorter edge two neighbouring screens keep (as the display controls do). */
export const EDGE_SHARE = 0.1;
/** Within this many logical pixels of lined-up edges or centred, it snaps there. */
const MAGNET = 60;

function overlaps(a: Rect, b: Rect) {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * The allowed place nearest to `want` for a screen of `size`: against an
 * edge of one of `others`, sharing at least EDGE_SHARE of the shorter edge
 * (GNOME refuses screens that only touch at a corner or float apart), not
 * overlapping any of them. Snaps to lined-up edges or centred when close.
 */
export function nearestAllowedPosition(
  others: Rect[],
  size: { width: number; height: number },
  want: { x: number; y: number },
): { x: number; y: number } {
  let best: { x: number; y: number; distance: number } | null = null;
  for (const r of others) {
    for (const side of ["left", "right", "above", "below"] as const) {
      const vertical = side === "left" || side === "right";
      const edge = vertical ? r.height : r.width;
      const own = vertical ? size.height : size.width;
      const start = vertical ? r.y : r.x;
      const keep = Math.max(1, Math.round(Math.min(edge, own) * EDGE_SHARE));
      let along = Math.min(Math.max(vertical ? want.y : want.x, start - own + keep), start + edge - keep);
      for (const magnet of [start, start + edge - own, start + (edge - own) / 2]) {
        if (Math.abs(along - magnet) <= MAGNET) {
          along = Math.round(magnet);
          break;
        }
      }
      const rect = {
        x: side === "left" ? r.x - size.width : side === "right" ? r.x + r.width : along,
        y: side === "above" ? r.y - size.height : side === "below" ? r.y + r.height : along,
        ...size,
      };
      if (others.some((o) => overlaps(rect, o))) continue;
      const distance = Math.hypot(rect.x - want.x, rect.y - want.y);
      if (!best || distance < best.distance) best = { x: rect.x, y: rect.y, distance };
    }
  }
  return best ? { x: best.x, y: best.y } : want;
}

export function updateDisplayPosition(layout: DisplayLayout, id: string, pos: { x: number; y: number }): DisplayLayout {
  if (isDuoDisplay(id)) {
    return normalizeDuoDisplays(layout);
  }

  const normalized = normalizeDuoDisplays(layout);
  const dragged = normalized.displays.find((d) => d.connector === id);
  if (!dragged) {
    return normalized;
  }
  const others = normalized.displays
    .filter((d) => d.connector !== id)
    .map((d) => ({ x: d.x, y: d.y, ...displayLogicalSize(d) }));
  const place = nearestAllowedPosition(others, displayLogicalSize(dragged), snapDisplayPosition(pos));
  const displays = normalized.displays.map((display) =>
    display.connector === id ? { ...display, x: Math.round(place.x), y: Math.round(place.y) } : display,
  );

  return normalizeDuoDisplays({ displays });
}

export function refreshSelectValue(display: DisplayInfo) {
  if (display.refreshPolicy === "dynamic") {
    return "dynamic";
  }
  return display.currentMode.modeId;
}

export function modesForResolution(display: DisplayInfo) {
  return display.availableModes.filter(
    (mode) => mode.width === display.currentMode.width && mode.height === display.currentMode.height,
  );
}
