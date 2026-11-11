import { Handle, type NodeProps, Position } from "@xyflow/react";
import { Link2 } from "lucide-react";
import { type C4FlowNode, typeTag } from "./c4Layout";
import { type C4Element, kindLabel } from "./parse";

const HANDLE = "!size-1.5 !border-0 !bg-transparent";

/** React Flow needs an end for each arrow; the arrows themselves follow their ELK route. */
function Handles() {
  return (
    <>
      <Handle type="target" position={Position.Top} className={HANDLE} />
      <Handle type="source" position={Position.Bottom} className={HANDLE} />
    </>
  );
}

/** Fill, border and text by kind, as the design boards draw them: the system solid, outside things dashed grey. */
function tone(element: C4Element): { box: string; muted: string } {
  const plain = { box: "text-card-foreground", muted: "text-muted-foreground" };
  if (element.external) return { ...plain, box: `${plain.box} border-dashed border-zinc-400 bg-zinc-500/10 dark:border-zinc-500` };
  if (element.kind === "person") return { ...plain, box: `${plain.box} border-zinc-400 bg-card dark:border-zinc-500` };
  // The system is solid blue with white text in both themes; the dark theme's own primary is too light for white text.
  if (element.kind === "system") return { box: "border-primary bg-primary text-primary-foreground dark:border-blue-500 dark:bg-blue-700 dark:text-white", muted: "text-primary-foreground/80 dark:text-white/80" };
  if (element.shape !== "box") return { ...plain, box: `${plain.box} border-zinc-400 bg-card dark:border-zinc-500` };
  return { ...plain, box: `${plain.box} border-primary bg-primary/10` };
}

/** One person, system, container or component: its name, its type in brackets and its description. */
export function C4Leaf({ data, selected }: NodeProps<C4FlowNode>) {
  const { element, faded, opens } = data;
  const colors = tone(element);
  return (
    <div
      data-kind={element.kind}
      data-external={element.external}
      className={`relative flex size-full cursor-pointer flex-col items-center justify-center gap-0.5 overflow-hidden border px-3 py-1.5 text-center transition-opacity ${colors.box} ${
        element.kind === "person" ? "rounded-[28px]" : element.shape === "db" ? "rounded-[14px/10px]" : "rounded-lg"
      } ${selected ? "ring-2 ring-ring" : ""} ${faded ? "opacity-35" : ""}`}
    >
      <p className="max-w-full break-words text-[13px] leading-[18px] font-semibold">{element.label}</p>
      <p className={`max-w-full break-words text-[11px] leading-[15px] ${colors.muted}`}>{typeTag(element)}</p>
      {element.description === undefined ? null : <p className="max-w-full break-words text-xs leading-4">{element.description}</p>}
      {opens ? <Link2 className="absolute top-1.5 right-1.5 size-3 shrink-0 opacity-80" aria-label="Opens the next level" /> : null}
      <Handles />
    </div>
  );
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A boundary, deployment node or element that wraps others; its name and type sit at the top. */
export function C4Group({ data, selected }: NodeProps<C4FlowNode>) {
  const { element } = data;
  const boundary = element.kind === "boundary" || element.kind === "node";
  return (
    <div
      data-kind={element.kind}
      className={`size-full cursor-pointer rounded-lg border ${
        boundary ? "border-dashed border-primary/70 bg-primary/[0.03]" : `${tone(element).box} !bg-transparent`
      } ${selected ? "ring-2 ring-ring" : ""}`}
    >
      <Handles />
    </div>
  );
}

/** The name and type of a boundary, in its top left corner. */
export function C4Name({ data }: NodeProps<C4FlowNode>) {
  const { element } = data;
  return (
    <p className="w-max whitespace-nowrap rounded bg-background/90 px-1 text-[13px] leading-4 font-semibold text-primary">
      {element.label} <span className="font-normal">[{capital(element.type ?? kindLabel(element))}]</span>
    </p>
  );
}
