import { useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
  useNodesState,
} from "@xyflow/react";

import type { DisplayLayout } from "@/types/duo";
import { cn } from "@/lib/utils";

interface DisplayCanvasProps {
  layout: DisplayLayout;
  onLayoutChange: (layout: DisplayLayout) => void;
}

type DisplayNodeData = {
  connector: string;
  width: number;
  height: number;
  logicalHeight: number;
  refreshRate: number;
  primary: boolean;
  scale: number;
};

const PX_SCALE = 0.12; // graph px per real display pixel
const MIN_NODE_W = 220;
const MIN_NODE_H = 120;
const TOP_DISPLAY_CONNECTOR = "eDP-1";
const BOTTOM_DISPLAY_CONNECTOR = "eDP-2";

function isDuoDisplay(connector: string) {
  return connector === TOP_DISPLAY_CONNECTOR || connector === BOTTOM_DISPLAY_CONNECTOR;
}

function snapDisplayPosition(layout: DisplayLayout, id: string, pos: { x: number; y: number }) {
  const displays = layout.displays;
  const d = displays.find((x) => x.connector === id);
  if (!d) return { x: Math.round(pos.x / PX_SCALE), y: Math.round(pos.y / PX_SCALE) };

  const x = Math.round(pos.x / PX_SCALE);
  const y = Math.round(pos.y / PX_SCALE);
  return { x, y };
}

function displayLogicalSize(display: DisplayLayout["displays"][number]) {
  const rotated = display.transform === 90 || display.transform === 270;
  const physicalWidth = rotated ? display.height : display.width;
  const physicalHeight = rotated ? display.width : display.height;
  const scale = Math.max(display.scale, 0.1);

  return {
    width: physicalWidth / scale,
    height: physicalHeight / scale,
  };
}

function stackedLogicalHeight(display: DisplayLayout["displays"][number]) {
  const rotated = display.transform === 90 || display.transform === 270;
  const physicalHeight = rotated ? display.width : display.height;
  const scale = Math.max(display.scale, 0.1);
  return Math.ceil(physicalHeight / scale);
}

function normalizeDuoDisplays(layout: DisplayLayout) {
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

function DisplayNode({ data, selected }: NodeProps<Node<DisplayNodeData>>) {
  return (
    <div
      className={cn(
        "h-full w-full rounded-xl border-2 bg-card/85 backdrop-blur-sm",
        "shadow-sm shadow-black/5",
        data.primary
          ? "border-primary/60 bg-primary/5 shadow-md shadow-primary/10"
          : selected
            ? "border-primary/50"
            : "border-border/70"
      )}
    >
      <div className="flex h-full flex-col justify-between p-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="truncate font-mono text-[13px] font-semibold">
              {data.connector}
            </div>
            <div className="mt-0.5 font-mono text-[11px] text-muted-foreground">
              {data.width}x{data.height} @ {data.refreshRate.toFixed(1)}Hz
            </div>
          </div>

          {data.primary && (
            <span className="shrink-0 rounded bg-primary/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-primary">
              Primary
            </span>
          )}
        </div>

        <div className="flex items-end justify-between gap-3">
          <div className="text-[10px] text-muted-foreground">
            Drag to reposition
          </div>
          <div className="font-mono text-[11px] text-muted-foreground">
            {data.scale.toFixed(2)}x
          </div>
        </div>
      </div>
    </div>
  );
}

const nodeTypes = { displayNode: DisplayNode };

function layoutToNodes(layout: DisplayLayout): Node<DisplayNodeData>[] {
  const normalizedLayout = normalizeDuoDisplays(layout);

  // If multiple displays share (x,y), stagger them for usability.
  const primary =
    normalizedLayout.displays.find((d) => d.primary) ?? normalizedLayout.displays[0];
  const baseX = primary?.x ?? 0;
  const baseY = primary?.y ?? 0;
  const primaryLogical = primary ? displayLogicalSize(primary) : null;

  const seen = new Map<string, number>();

  return normalizedLayout.displays.map((d) => {
    const logical = displayLogicalSize(d);
    const key = `${d.x},${d.y}`;
    const count = seen.get(key) ?? 0;
    seen.set(key, count + 1);

    // If this (x,y) collides, stagger below the primary using logical display sizes.
    const staggerY = count === 0 ? 0 : Math.floor((primaryLogical?.height ?? logical.height) * count);
    const effectiveX = count === 0 ? d.x : baseX;
    const effectiveY = count === 0 ? d.y : baseY + staggerY;

    const scaledWidth = Math.round(logical.width * PX_SCALE);
    const scaledHeight = Math.round(logical.height * PX_SCALE);
    const w = Math.max(MIN_NODE_W, scaledWidth);
    const h = isDuoDisplay(d.connector)
      ? scaledHeight
      : Math.max(MIN_NODE_H, scaledHeight);

    return {
      id: d.connector,
      type: "displayNode",
      position: { x: effectiveX * PX_SCALE, y: effectiveY * PX_SCALE },
      draggable: !isDuoDisplay(d.connector),
      selectable: true,
      data: {
        connector: d.connector,
        width: d.width,
        height: d.height,
        logicalHeight: logical.height,
        refreshRate: d.refreshRate,
        primary: d.primary,
        scale: d.scale,
      },
      style: {
        width: w,
        height: h,
      },
    };
  });
}

function updateDisplayPosition(layout: DisplayLayout, id: string, pos: { x: number; y: number }) {
  if (isDuoDisplay(id)) {
    return normalizeDuoDisplays(layout);
  }

  const snapped = snapDisplayPosition(layout, id, pos);
  const displays = layout.displays.map((d) => {
    if (d.connector === id) {
      return { ...d, x: snapped.x, y: snapped.y };
    }

    return d;
  });
  return normalizeDuoDisplays({ displays });
}

export default function DisplayCanvas({ layout, onLayoutChange }: DisplayCanvasProps) {
  const rfRef = useRef<ReactFlowInstance<Node<DisplayNodeData>> | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const derivedNodes = useMemo(() => layoutToNodes(layout), [layout]);
  const [nodes, setNodes, onNodesChange] = useNodesState<Node<DisplayNodeData>>([]);

  // Keep nodes in sync with layout, but don't clobber while the user is dragging.
  useEffect(() => {
    if (isDragging) return;
    setNodes(derivedNodes);
  }, [derivedNodes, isDragging, setNodes]);

  // Fit view when layout changes (refresh/apply), but avoid fighting during drag.
  useEffect(() => {
    if (isDragging) return;
    const inst = rfRef.current;
    if (!inst) return;
    if (derivedNodes.length === 0) return;
    // Small delay lets ReactFlow measure node sizes.
    const t = window.setTimeout(() => {
      try {
        inst.fitView({ padding: 0.2, duration: 250 });
      } catch {
        // ignore
      }
    }, 0);
    return () => window.clearTimeout(t);
  }, [derivedNodes.length, isDragging]);

  return (
    <div className="h-[420px] overflow-hidden rounded-xl border border-border/60 bg-muted/20">
      <ReactFlow
        nodes={nodes}
        edges={[]}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onInit={(inst: ReactFlowInstance<Node<DisplayNodeData>>) => {
          rfRef.current = inst;
        }}
        nodesConnectable={false}
        nodesDraggable
        elementsSelectable
        fitView
        snapToGrid
        snapGrid={[10, 10]}
        onNodeDragStart={(_, node) => {
          if (isDuoDisplay(node.id)) return;
          setIsDragging(true);
        }}
        onNodeDrag={(_, node: Node<DisplayNodeData>) => {
          if (isDuoDisplay(node.id)) return;
          setNodes((nds) =>
            nds.map((n) => (n.id === node.id ? { ...n, position: node.position } : n))
          );
        }}
        onNodeDragStop={(_, node: Node<DisplayNodeData>) => {
          if (isDuoDisplay(node.id)) {
            setIsDragging(false);
            return;
          }
          setIsDragging(false);
          const next = updateDisplayPosition(layout, node.id, node.position);
          onLayoutChange(next);
        }}
        onPaneClick={() => {
          // ensure drag state resets if something weird happens
          setIsDragging(false);
        }}
      >
        <Background gap={20} size={1} color="oklch(0.5 0 0 / 10%)" />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
