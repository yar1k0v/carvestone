import { BaseEdge, type EdgeProps } from "@xyflow/react";
import type { C4FlowEdge } from "./c4Layout";

/**
 * An arrow along the route ELK found, so it bends around boxes instead of crossing them. Its label
 * is drawn in the box the layout kept free for it, one line per row, with a page-coloured ground.
 */
export function C4Edge({ data, label, style, markerEnd, markerStart, labelStyle }: EdgeProps<C4FlowEdge>) {
  const points = data?.points ?? [];
  const path = points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  const box = data?.labelBox;
  const lines = data?.lines ?? [];
  return (
    <>
      <BaseEdge
        path={path}
        {...(style === undefined ? {} : { style })}
        {...(markerEnd === undefined ? {} : { markerEnd })}
        {...(markerStart === undefined ? {} : { markerStart })}
      />
      {label === undefined || box === undefined || lines.length === 0 ? null : (
        <g aria-hidden="true" className="pointer-events-none" style={{ opacity: labelStyle?.opacity ?? 1 }}>
          <rect x={box.x} y={box.y} width={box.width} height={box.height} rx={3} style={{ fill: "var(--background)", fillOpacity: 0.92 }} />
          <text textAnchor="middle" fontSize={11} style={{ fill: labelStyle?.fill ?? "currentColor" }}>
            {lines.map((line, at) => (
              <tspan key={line + at} x={box.x + box.width / 2} y={box.y + 13 + at * 14}>
                {line}
              </tspan>
            ))}
          </text>
        </g>
      )}
    </>
  );
}
