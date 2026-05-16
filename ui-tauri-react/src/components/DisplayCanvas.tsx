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
import {
  isDuoDisplay,
  layoutToCanvasNodes,
  updateDisplayPosition,
  type DisplayCanvasNodeModel,
} from "@/lib/display-layout";
import { cn } from "@/lib/utils";

interface DisplayCanvasProps {
  layout: DisplayLayout;
  onLayoutChange: (layout: DisplayLayout) => void;
}

type DisplayNodeData = DisplayCanvasNodeModel["data"];

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
  return layoutToCanvasNodes(layout).map((node) => ({
    ...node,
    type: "displayNode",
    selectable: true,
  }));
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
