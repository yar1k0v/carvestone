import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { api } from "@/api/client";
import { keys } from "@/api/keys";
import type { ListRow } from "@/api/types/ListRow";
import type { PatchReply } from "@/api/types/PatchReply";
import type { PatchRequest } from "@/api/types/PatchRequest";
import { useHld } from "@/components/graph/useDesign";
import { itemUrl } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { useRowsOfType } from "@/components/planning/useRows";
import { useProduct } from "@/components/product/useProduct";
import { Button } from "@/components/ui/button";
import { foundCounts, hidden, hide, prdOf } from "./firstRunModel";

/** What the agent is asked to do; fixed text, so the owner can read it before pasting. */
const PROMPT =
  "Read the product requirements with `stone get docs/prd`. Make one doc per feature with `stone new doc --set kind=feature`, each with a title and a short summary taken from the PRD. Then stop, so the owner can review the drafts.";

/** The board's soft blue action, for a step's button. */
const ACTION = "h-auto border-0 bg-st-progress-soft px-3 py-1.5 text-[13px] font-medium text-st-progress hover:bg-st-progress-soft/70";

function Step({ title, children, action }: { title: string; children: ReactNode; action: ReactNode }) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t px-4 py-3 first:border-t-0">
      <div className="min-w-0 flex-[1_1_260px]">
        <div className="text-[15px] font-semibold">{title}</div>
        <div className="text-muted-foreground">{children}</div>
      </div>
      <div className="shrink-0">{action}</div>
    </li>
  );
}

/** Makes the PRD the product overview: the doc gets the kind the product page reads. */
function UsePrd({ prd }: { prd: ListRow }) {
  const client = useQueryClient();
  const write = useMutation({
    mutationFn: () => {
      const request: PatchRequest = { set: { kind: "product-overview" }, if_match: prd.etag };
      return api.patch<PatchReply>(itemUrl(prd.id), request);
    },
    onSuccess: async () => {
      await Promise.all([client.invalidateQueries({ queryKey: ["items"] }), client.invalidateQueries({ queryKey: keys.product() })]);
    },
  });
  return (
    <>
      <Button size="sm" className={ACTION} disabled={write.isPending} onClick={() => write.mutate()}>
        Use PRD
      </Button>
      {write.isError ? (
        <p role="alert" className="mt-1 max-w-xs text-xs text-destructive">
          {write.error.message}
        </p>
      ) : null}
    </>
  );
}

/** Copies the drafting prompt; where the browser refuses, the prompt is shown to copy by hand. */
function CopyPrompt() {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(PROMPT);
      setState("copied");
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setState("idle"), 2000);
    } catch {
      setState("failed");
    }
  };
  return (
    <>
      <Button size="sm" className={ACTION} onClick={copy}>
        {state === "copied" ? "Copied" : "Copy prompt"}
      </Button>
      {state === "failed" ? <pre className="mt-2 max-w-xs whitespace-pre-wrap rounded-md border p-2 text-xs">{PROMPT}</pre> : null}
    </>
  );
}

/**
 * What stone found in a project that has no product overview yet, and the next steps. Left out while
 * the product loads or fails, once there is an overview, and after the owner hides it.
 */
export function FirstRun({ tasks, milestones }: { tasks: ListRow[]; milestones: ListRow[] }) {
  const product = useProduct();
  const hld = useHld();
  const decisions = useRowsOfType("decision");
  const docs = useRowsOfType("doc");
  const [gone, setGone] = useState(hidden);
  if (!product.data || product.data.overview !== null || gone) return null;

  const docRows = docs.data ?? [];
  const prd = prdOf(docRows);
  const services = hld.data?.containers.length ?? 0;
  return (
    <section aria-labelledby="first-run" className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 id="first-run" className="text-xl font-semibold">
            Stone already knows this much
          </h2>
          <p className="text-muted-foreground">Found in this repository. Finish these steps to fill in the rest.</p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            hide();
            setGone(true);
          }}
        >
          Hide
        </Button>
      </div>
      <div className="grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(150px,1fr))]">
        {foundCounts(tasks, milestones, decisions.data ?? [], docRows).map((found) => (
          <div key={found.label} className="slab flex flex-col px-3.5 py-3">
            <span className="text-2xl font-[650] tabular-nums">{found.count}</span>
            <span className="text-muted-foreground">{found.label}</span>
          </div>
        ))}
      </div>
      <ul className="slab">
        {prd ? (
          <Step title="Use the PRD as the product overview" action={<UsePrd prd={prd} />}>
            {prd.title ?? prd.id} becomes the overview on the Product page.
          </Step>
        ) : null}
        <Step title="List your features" action={<CopyPrompt />}>
          Ask your agent to draft one feature doc per feature from the PRD, for you to confirm.
        </Step>
        <Step
          title="Name your services"
          action={
            <AppLink href="/architecture" className={`${ACTION} inline-flex rounded-[7px]`}>
              Review services
            </AppLink>
          }
        >
          {services} services are on the System map.
        </Step>
      </ul>
    </section>
  );
}
