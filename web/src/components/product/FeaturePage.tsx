import { useQuery } from "@tanstack/react-query";
import { useLocation } from "@tanstack/react-router";
import type { ReactNode } from "react";
import type { FeatureDetail } from "@/api/types/FeatureDetail";
import type { FeatureTask } from "@/api/types/FeatureTask";
import type { ItemView } from "@/api/types/ItemView";
import { Empty, Tag } from "@/components/architecture/service/parts";
import { readDoc } from "@/components/docs/lead";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { isItemView, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { serviceHref } from "@/components/graph/serviceHref";
import { hrefFor, taskHref } from "@/components/markdown/links";
import { useGo, useResolver } from "@/components/markdown/useResolver";
import { More, PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { StatusBadge, toneRank } from "@/components/ui/status";
import { CommitRows } from "@/components/work/CommitRows";
import { isOver } from "@/components/work/taskModel";
import { cn } from "@/lib/utils";
import { useCrumbs, useOutline } from "@/shell/pageFrame";
import { Technical } from "./Technical";
import { type FeatureTab, featureDocId } from "./model";
import { featureQuery } from "./useProduct";

const TASKS_SHOWN = 15;

function WrittenDoc({ item, title }: { item: ItemView; title: string }) {
  const resolver = useResolver();
  const go = useGo();
  const hash = useLocation({ select: (location) => location.hash });
  const { body } = readDoc(item.body, title);
  if (body.trim() === "") return <Empty>The feature doc has no text yet. Say what it is and which rules it follows.</Empty>;
  return <Markdown source={body} resolver={resolver} fromId={item.id} onNavigate={go} activeAnchor={hash} />;
}

function Docs({ detail }: { detail: FeatureDetail }) {
  const docs = detail.links.docs;
  if (docs.length === 0) return <Empty>No other doc or decision links this feature, and it links none.</Empty>;
  return (
    <ul aria-label="Documents and decisions" className="slab divide-y overflow-hidden">
      {docs.map((doc) => (
        <li key={doc.id} className="flex items-baseline gap-3 px-3.5 py-2 text-sm">
          <AppLink href={hrefFor(doc.id)} className="min-w-0 flex-1 font-medium hover:text-primary">
            {doc.title}
          </AppLink>
          <Tag>{doc.type}</Tag>
        </li>
      ))}
    </ul>
  );
}

function TaskRow({ task }: { task: FeatureTask }) {
  return (
    <li className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 px-3.5 py-2 text-sm">
      <span className="w-16 shrink-0 font-mono text-xs text-muted-foreground">{task.id.replace(/^tasks\//, "")}</span>
      <AppLink href={taskHref(task.id)} className="min-w-0 flex-1 basis-60 font-medium hover:text-primary">
        {task.title}
      </AppLink>
      {task.inferred ? <Tag title={task.via === undefined ? "Nothing links the two." : `Nothing links the two. They share ${task.via}.`}>inferred</Tag> : null}
      {task.closed === undefined || task.closed_estimated ? null : <span className="text-xs text-muted-foreground tabular-nums">{task.closed}</span>}
      <StatusBadge status={task.status ?? null} />
    </li>
  );
}

function Tasks({ tasks }: { tasks: FeatureTask[] }) {
  if (tasks.length === 0) return <Empty>No tasks are tied to this feature yet. A task shows here when it links the feature doc, or shares a link, a tag or a code path with it.</Empty>;
  const sorted = [...tasks].sort((a, b) => toneRank(a.status) - toneRank(b.status));
  return (
    <More rows={sorted} limit={TASKS_SHOWN}>
      {(shown) => (
        <ul aria-label="Tasks" className="slab divide-y overflow-hidden">
          {shown.map((task) => (
            <TaskRow key={task.id} task={task} />
          ))}
        </ul>
      )}
    </More>
  );
}

const TAB_LABELS: { tab: FeatureTab | undefined; label: string }[] = [
  { tab: undefined, label: "Behaviour" },
  { tab: "technical", label: "Technical" },
  { tab: "docs", label: "Decisions and docs" },
  { tab: "tasks", label: "Tasks" },
  { tab: "history", label: "History" },
];

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <p className="text-xs text-muted-foreground">{label}</p>
      <div className="font-medium">{children}</div>
    </div>
  );
}

function Facts({ detail, openTasks, onTab }: { detail: FeatureDetail; openTasks: number; onTab: (tab: FeatureTab | undefined) => void }) {
  const services = detail.links.parts.filter((part) => part.kind === "container" || part.kind === "system");
  return (
    <aside aria-label="Feature facts" className="slab grid min-w-0 flex-[1_1_280px] gap-3 self-start px-[18px] py-4 text-sm leading-[1.45]">
      {detail.status === undefined ? null : (
        <Fact label="State">
          <Tag>{detail.status}</Tag>
        </Fact>
      )}
      {detail.platforms.length === 0 ? null : <Fact label="Available on">{detail.platforms.join(", ")}</Fact>}
      {openTasks === 0 ? null : (
        <Fact label="Open work">
          <button type="button" onClick={() => onTab("tasks")} className="font-medium text-primary hover:underline">
            {openTasks} {openTasks === 1 ? "task" : "tasks"}
          </button>
        </Fact>
      )}
      {services.length === 0 ? null : (
        <Fact label="Services">
          {services.map((part, at) => (
            <span key={part.id}>
              {at === 0 ? "" : ", "}
              <AppLink href={serviceHref(part.id)} className="text-primary hover:underline">
                {part.title}
              </AppLink>
            </span>
          ))}
        </Fact>
      )}
      {detail.updated === undefined ? null : <Fact label="Last updated">{detail.updated}</Fact>}
    </aside>
  );
}

function Feature({ detail, item, tab, onTab }: { detail: FeatureDetail; item: ItemView | null; tab: FeatureTab | undefined; onTab: (tab: FeatureTab | undefined) => void }) {
  useCrumbs([{ label: "Product", href: "/product" }, { label: detail.title }]);
  useOutline([]);
  const openTasks = detail.links.tasks.filter((task) => !isOver(task.status)).length;
  const current = TAB_LABELS.find((entry) => entry.tab === tab) ?? TAB_LABELS[0];
  return (
    <article className="flex flex-wrap gap-7 pb-8">
      <div className="flex min-w-0 flex-[999_1_520px] flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <h1 className="text-[26px] leading-8 font-[650] tracking-[-0.02em]">{detail.title}</h1>
          {detail.summary === undefined ? null : <p className="max-w-[62ch] text-base text-foreground/85">{detail.summary}</p>}
        </div>
        <div className="flex flex-col gap-4 text-sm leading-[1.45]">
          <div role="tablist" aria-label="Feature sections" className="flex flex-wrap gap-5 border-b">
            {TAB_LABELS.map((entry) => {
              const selected = entry === current;
              return (
                <button
                  key={entry.label}
                  type="button"
                  role="tab"
                  id={`feature-tab-${entry.tab ?? "behaviour"}`}
                  aria-selected={selected}
                  aria-controls="feature-panel"
                  onClick={() => onTab(entry.tab)}
                  className={cn("py-[7px]", selected ? "border-b-2 border-primary font-semibold text-primary" : "text-muted-foreground hover:text-foreground")}
                >
                  {entry.label}
                  {entry.tab === "tasks" ? ` ${openTasks}` : ""}
                </button>
              );
            })}
          </div>
          <div role="tabpanel" id="feature-panel" aria-labelledby={`feature-tab-${current?.tab ?? "behaviour"}`}>
            {current?.tab === undefined ? (
              item === null ? (
                <Loading lines={6} label="Reading the feature doc..." className="max-w-3xl" />
              ) : (
                <WrittenDoc item={item} title={detail.title} />
              )
            ) : null}
            {current?.tab === "technical" ? <Technical detail={detail} /> : null}
            {current?.tab === "docs" ? <Docs detail={detail} /> : null}
            {current?.tab === "tasks" ? <Tasks tasks={detail.links.tasks} /> : null}
            {current?.tab === "history" ? (
              detail.links.commits.length === 0 ? (
                <Empty>No commits yet. They show here once a task of this feature has commits.</Empty>
              ) : (
                <CommitRows commits={detail.links.commits} />
              )
            ) : null}
          </div>
        </div>
      </div>
      <Facts detail={detail} openTasks={openTasks} onTab={onTab} />
    </article>
  );
}

/** One feature, by the path after `/features/`: what it is, how it is built, and the work on it. */
export function FeaturePage({ target, tab, onTab }: { target: string; tab: FeatureTab | undefined; onTab: (tab: FeatureTab | undefined) => void }) {
  const id = featureDocId(target);
  const feature = useQuery(featureQuery(id));
  const doc = useItemQuery(id);
  const item = doc.data !== undefined && isItemView(doc.data) ? doc.data : null;
  if (feature.isPending) {
    return (
      <>
        <PageHeader title={target} />
        <Loading lines={6} className="max-w-3xl" />
      </>
    );
  }
  if (feature.isError) {
    return (
      <>
        <PageHeader title={target} />
        <ErrorNotice error={feature.error} />
      </>
    );
  }
  return <Feature detail={feature.data} item={item} tab={tab} onTab={onTab} />;
}
