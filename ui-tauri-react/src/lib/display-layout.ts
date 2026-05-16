import type { DisplayInfo, DisplayLayout } from "@/types/duo";

export const DISPLAY_CANVAS_SCALE = 0.12;
export const MIN_DISPLAY_NODE_WIDTH = 220;
export const MIN_DISPLAY_NODE_HEIGHT = 120;
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
  const normalizedLayout = normalizeDuoDisplays(layout);
  const primary = normalizedLayout.displays.find((d) => d.primary) ?? normalizedLayout.displays[0];
  const baseX = primary?.x ?? 0;
  const baseY = primary?.y ?? 0;
  const primaryLogical = primary ? displayLogicalSize(primary) : null;
  const seen = new Map<string, number>();

  return normalizedLayout.displays.map((display) => {
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
      style: {
        width: Math.max(MIN_DISPLAY_NODE_WIDTH, scaledWidth),
        height: isDuoDisplay(display.connector)
          ? scaledHeight
          : Math.max(MIN_DISPLAY_NODE_HEIGHT, scaledHeight),
      },
    };
  });
}

export function updateDisplayPosition(layout: DisplayLayout, id: string, pos: { x: number; y: number }): DisplayLayout {
  if (isDuoDisplay(id)) {
    return normalizeDuoDisplays(layout);
  }

  const snapped = snapDisplayPosition(pos);
  const displays = layout.displays.map((display) =>
    display.connector === id ? { ...display, x: snapped.x, y: snapped.y } : display,
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
