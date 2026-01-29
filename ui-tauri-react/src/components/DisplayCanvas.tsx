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
  refreshRate: number;
  primary: boolean;
  scale: number;
};

const PX_SCALE = 0.12; // graph px per real display pixel
const MIN_NODE_W = 220;
const MIN_NODE_H = 140;

function snapVerticalPosition(layout: DisplayLayout, id: string, pos: { x: number; y: number }) {
  const displays = layout.displays;
  const d = displays.find((x) => x.connector === id);
  if (!d) return { x: Math.round(pos.x / PX_SCALE), y: Math.round(pos.y / PX_SCALE) };

  const primary = displays.find((x) => x.primary) ?? displays[0];
  const targetX = primary?.x ?? 0;

  const currentY = Math.round(pos.y / PX_SCALE);

  const candidates: number[] = [];
  for (const o of displays) {
    if (o.connector === id) continue;
    const above = o.y - d.height;
    if (above >= 0) candidates.push(above);
    candidates.push(o.y + o.height); // below
  }

  if (candidates.length === 0) {
    return { x: targetX, y: currentY };
  }

  let bestY = candidates[0];
  let bestDist = Math.abs(currentY - bestY);
  for (let i = 1; i < candidates.length; i++) {
    const y = candidates[i];
    const dist = Math.abs(currentY - y);
    if (dist < bestDist) {
      bestDist = dist;
      bestY = y;
    }
  }

  return { x: targetX, y: Math.max(0, bestY) };
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
  // If multiple displays share (x,y), stagger them for usability.
  const primary = layout.displays.find((d) => d.primary) ?? layout.displays[0];
  const baseX = primary?.x ?? 0;
  const baseY = primary?.y ?? 0;

  const seen = new Map<string, number>();

  return layout.displays.map((d) => {
    const key = `${d.x},${d.y}`;
    const count = seen.get(key) ?? 0;
    seen.set(key, count + 1);

    // If this (x,y) collides, stagger below the primary using real pixel sizes.
    const staggerY = count === 0 ? 0 : (primary?.height ?? d.height) * count;
    const effectiveX = count === 0 ? d.x : baseX;
    const effectiveY = count === 0 ? d.y : baseY + staggerY;

    const w = Math.max(MIN_NODE_W, Math.round(d.width * PX_SCALE));
    const h = Math.max(MIN_NODE_H, Math.round(d.height * PX_SCALE));

    return {
      id: d.connector,
      type: "displayNode",
      position: { x: effectiveX * PX_SCALE, y: effectiveY * PX_SCALE },
      draggable: true,
      selectable: true,
      data: {
        connector: d.connector,
        width: d.width,
        height: d.height,
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
  const snapped = snapVerticalPosition(layout, id, pos);
  const displays = layout.displays.map((d) => {
    if (d.connector !== id) return d;
    return { ...d, x: snapped.x, y: snapped.y };
  });
  return { displays };
}

export default function DisplayCanvas({ layout, onLayoutChange }: DisplayCanvasProps) {
  const rfRef = useRef<ReactFlowInstance<Node<DisplayNodeData>> | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const fixedX = useMemo(() => {
    const primary = layout.displays.find((d) => d.primary) ?? layout.displays[0];
    return (primary?.x ?? 0) * PX_SCALE;
  }, [layout.displays]);

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
        onNodeDragStart={() => setIsDragging(true)}
        onNodeDrag={(_, node: Node<DisplayNodeData>) => {
          // Physical Duo panels are stacked; keep X locked so nodes only move up/down.
          setNodes((nds) =>
            nds.map((n) =>
              n.id === node.id
                ? {
                    ...n,
                    position: { x: fixedX, y: node.position.y },
                  }
                : n
            )
          );
        }}
        onNodeDragStop={(_, node: Node<DisplayNodeData>) => {
          setIsDragging(false);
          const constrained = { x: fixedX, y: node.position.y };
          const next = updateDisplayPosition(layout, node.id, constrained);
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
