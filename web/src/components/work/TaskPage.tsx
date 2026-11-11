import { useLocation } from "@tanstack/react-router";
import { type ReactNode, useMemo } from "react";
import type { ItemView } from "@/api/types/ItemView";
import type { ListRow } from "@/api/types/ListRow";
import { Answer, Block, Empty, Tag, link } from "@/components/architecture/service/parts";
import { type Connection, connectionsOf } from "@/components/docs/connections";
import { dropTitle } from "@/components/docs/lead";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { ChangedFiles } from "@/components/item/Extras";
import { isItemView, useCachedRow, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { type Resolver, taskHref } from "@/components/markdown/links";
import { useGo, useItemRows, useResolver } from "@/components/markdown/useResolver";
import { textField } from "@/components/planning/progress";
import { featureHref } from "@/components/product/model";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { PriorityBadge, StatusBadge } from "@/components/ui/status";
import { useCrumbs, useOutline } from "@/shell/pageFrame";
import { CommitRows } from "./CommitRows";
import { Decision, StatusPicker } from "./OwnerActions";
import { blockedIds, isOver, readTask, refTarget, textList, useTaskCommits } from "./taskModel";
import { useTaskFeatures } from "./useTaskFeatures";

const OUTLINE = [
  { id: "task-commits", title: "Commits" },
  { id: "task-docs", title: "Docs" },
  { id: "task-architecture", title: "Architecture" },
  { id: "task-text", title: "Task text" },
];

/** One line of the top of the page. */
function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-x-4 gap-y-0.5 px-4 py-2.5 sm:grid-cols-[8rem_1fr]">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd aria-label={label} className="min-w-0 text-sm">
        {children}
      </dd>
    </div>
  );
}

function None({ children }: { children: ReactNode }) {
  return <span className="text-muted-foreground">{children}</span>;
}

function TaskRef({ id, row }: { id: string; row: ListRow | undefined }) {
  return (
    <li className="flex items-baseline gap-2 py-0.5">
      <span className="shrink-0 font-mono text-xs text-muted-foreground">{row?.short ?? id.replace(/^tasks\//, "")}</span>
      <AppLink href={taskHref(id)} className={`min-w-0 flex-1 ${link}`}>
        {row?.title ?? id}
      </AppLink>
      <StatusBadge status={row?.status ?? null} />
    </li>
  );
}

function Left({ item, status }: { item: ItemView; status: string | null }) {
  const { checks } = readTask(item.body);
  if (isOver(status)) return <None>Nothing. This task is {status}.</None>;
  const open = checks.filter((check) => !check.checked);
  if (checks.length === 0) return <None>Not written down. The task has no checklist.</None>;
  if (open.length === 0) return <None>All {checks.length} checklist items are ticked.</None>;
  return (
    <>
      <p className="mb-1 text-muted-foreground tabular-nums">
        {open.length} of {checks.length} checklist items open
      </p>
      <ul className="list-disc space-y-0.5 pl-4">
        {open.map((check) => (
          <li key={check.text}>{check.text.replace(/\*\*|`/g, "")}</li>
        ))}
      </ul>
    </>
  );
}

function BlockedBy({ item, resolver, byId }: { item: ItemView; resolver: Resolver; byId: Map<string, ListRow> }) {
  const named = textList(item.fields.depends_on).map((target) => {
    const id = resolver.wiki(target)?.id ?? target;
    return { id, row: byId.get(id) };
  });
  const waiting = named.filter(({ row }) => !isOver(row?.status));
  const finished = named.length - waiting.length;
  if (named.length === 0) return <None>Nothing.</None>;
  return (
    <>
      {waiting.length === 0 ? <None>Nothing now.</None> : null}
      <ul>
        {waiting.map(({ id, row }) => (
          <TaskRef key={id} id={id} row={row} />
        ))}
      </ul>
      {finished === 0 ? null : <None> {finished === 1 ? "1 task it needed is finished." : `${finished} tasks it needed are finished.`}</None>}
    </>
  );
}

function Blocks({ item, byId }: { item: ItemView; byId: Map<string, ListRow> }) {
  const ids = blockedIds(item);
  if (ids.length === 0) return <None>Nothing.</None>;
  return (
    <ul>
      {ids.map((id) => (
        <TaskRef key={id} id={id} row={byId.get(id)} />
      ))}
    </ul>
  );
}

function Features({ item }: { item: ItemView }) {
  const features = useTaskFeatures(item.short ?? item.id);
  if (features.rows.length > 0) {
    return (
      <ul>
        {features.rows.map((feature) => (
          <li key={feature.id} className="flex items-baseline gap-2 py-0.5">
            <AppLink href={featureHref(feature.id)} className={link}>
              {feature.title}
            </AppLink>
            {feature.via === undefined ? null : <Tag title={`Nothing links the two. They share ${feature.via}.`}>Related</Tag>}
          </li>
        ))}
      </ul>
    );
  }
  if (features.isPending) return <None>Looking for its feature...</None>;
  return <None>{features.none ? "This project lists no features yet." : "None. Link the task to a feature doc."}</None>;
}

/** A milestone or plan field, as a link to its page when it resolves. */
export function WorkRef({ value, store, resolver, empty, className = link }: { value: string | null; store: string; resolver: Resolver; empty: string; className?: string }) {
  if (value === null) return <None>{empty}</None>;
  const found = resolver.wiki(refTarget(value));
  if (found === null) return <>{refTarget(value)}</>;
  return (
    <AppLink href={`/${store}/${found.id.replace(new RegExp(`^${store}/`), "")}`} className={className}>
      {found.title ?? found.id}
    </AppLink>
  );
}

function Links({ label, rows }: { label: string; rows: Connection[] }) {
  return (
    <ul aria-label={label} className="slab divide-y overflow-hidden">
      {rows.map((row) => (
        <li key={row.key} className="px-3.5 py-2 text-sm">
          <AppLink href={row.href} title={row.hint} className="font-medium hover:text-primary">
            {row.title}
          </AppLink>
          <p className="text-muted-foreground">{row.relation}</p>
        </li>
      ))}
    </ul>
  );
}

// The write invalidates the item, so this page reads the new status without help.
const refreshed = () => undefined;

function Task({ item }: { item: ItemView }) {
  const rows = useItemRows();
  const resolver = useResolver();
  const go = useGo();
  const hash = useLocation({ select: (location) => location.hash });
  useOutline(OUTLINE);

  const all = useMemo(() => rows.data ?? [], [rows.data]);
  const byId = useMemo(() => new Map(all.map((row) => [row.id, row])), [all]);
  const title = textField(item.fields, "title") ?? item.id;
  const status = textField(item.fields, "status");
  const body = dropTitle(item.body, title);
  const { goal } = readTask(body);
  const { documents, architecture } = useMemo(
    () =>
      connectionsOf({
        self: item.id,
        facts: [],
        links: [...textList(item.fields.links), ...textList(item.fields.components)],
        body,
        backlinks: item.backlinks.filter((row) => row.kind !== "depends_on"),
        rows: all,
        resolver,
      }),
    [item, body, all, resolver],
  );
  const commits = useTaskCommits(item.id);
  const changed = item.changed_files ?? [];
  const closed = textField(item.fields, "closed");
  const assignee = textField(item.fields, "assignee");

  return (
    <article className="pb-8">
      <PageHeader eyebrow={item.short ?? item.id} title={title} className="mb-4" />
      <dl aria-label="Task summary" className="slab divide-y">
        <Fact label="Status">
          <span className="flex flex-wrap items-center gap-2">
            <StatusBadge status={status} />
            <StatusPicker item={item} onDone={refreshed} />
            <PriorityBadge priority={textField(item.fields, "priority")} />
            {assignee === null ? null : <span className="text-muted-foreground">{assignee}</span>}
            {closed === null ? null : <span className="text-muted-foreground">Closed {closed}</span>}
          </span>
        </Fact>
        <Fact label="Goal">{goal === null ? <None>Not written down.</None> : <Markdown source={goal} resolver={resolver} fromId={item.id} onNavigate={go} compact />}</Fact>
        <Fact label="What is left">
          <Left item={item} status={status} />
        </Fact>
        <Fact label="Blocked by">
          <BlockedBy item={item} resolver={resolver} byId={byId} />
        </Fact>
        <Fact label="Blocks">
          <Blocks item={item} byId={byId} />
        </Fact>
        <Fact label="Feature">
          <Features item={item} />
        </Fact>
        <Fact label="Milestone">
          <WorkRef value={textField(item.fields, "milestone")} store="milestones" resolver={resolver} empty="None." />
        </Fact>
        <Fact label="Plan">
          <WorkRef value={textField(item.fields, "plan")} store="plans" resolver={resolver} empty="None." />
        </Fact>
      </dl>
      {status === "review" ? (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Decision task={{ id: item.id, etag: item.etag, status }} onDone={refreshed} />
        </div>
      ) : null}
      <p className="mt-2 mb-6 text-sm">
        <AppLink href={`/items/${item.id}`} className={link}>
          Open as item
        </AppLink>
      </p>
      <Block id="task-commits" title="Commits" lead="Each commit says how it was tied to this task.">
        <Answer query={commits} label="Reading the commits...">
          {(list) => (list.length === 0 ? <Empty>No commit names this task, and none changed the files it works on.</Empty> : <CommitRows commits={list} />)}
        </Answer>
      </Block>
      <Block id="task-docs" title="Docs">
        {documents.length === 0 ? <Empty>No doc links this task, and it links none.</Empty> : <Links label="Docs" rows={documents} />}
      </Block>
      <Block id="task-architecture" title="Architecture">
        {architecture.length === 0 && changed.length === 0 ? <Empty>This task names no part of the architecture, and no changed files are recorded.</Empty> : null}
        {architecture.length === 0 ? null : <Links label="Architecture" rows={architecture} />}
        {changed.length === 0 ? null : (
          <div className="mt-4">
            <h3 className="mb-2 text-sm font-medium">Changed files</h3>
            <ChangedFiles files={changed} />
          </div>
        )}
      </Block>
      <Block id="task-text" title="Task text">
        {body.trim() === "" ? <Empty>The task has no text.</Empty> : <Markdown source={body} resolver={resolver} fromId={item.id} onNavigate={go} activeAnchor={hash} />}
      </Block>
    </article>
  );
}

/** One task by the id after `/tasks/`: where it stands, then what it connects to. */
export function TaskPage({ target }: { target: string }) {
  const id = `tasks/${target}`;
  const query = useItemQuery(id);
  const cached = useCachedRow(id);
  const view = query.data !== undefined && isItemView(query.data) ? query.data : null;
  const milestone = cached?.milestone ?? (view === null ? null : textField(view.fields, "milestone"));
  useCrumbs([
    { label: "Work" },
    ...(milestone === null ? [{ label: "Board", href: "/board" }] : [{ label: milestone.replace(/^milestones\//, ""), href: `/milestones/${milestone.replace(/^milestones\//, "")}` }]),
    { label: view === null ? (cached?.title ?? target) : (textField(view.fields, "title") ?? target) },
  ]);
  if (query.isPending) {
    return (
      <article>
        <PageHeader eyebrow={cached?.short ?? target} title={cached?.title ?? target} className="mb-4" />
        <Loading lines={8} className="max-w-3xl" />
      </article>
    );
  }
  if (query.isError) return <ErrorNotice error={query.error} />;
  if (view === null) return <ErrorNotice error={new Error(`${target} is not a task`)} />;
  return <Task item={view} />;
}
