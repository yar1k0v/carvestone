import { type ReactNode, useMemo, useState } from "react";
import type { ItemView } from "@/api/types/ItemView";
import type { ListRow } from "@/api/types/ListRow";
import { Answer, Empty } from "@/components/architecture/service/parts";
import { columnLabel, ageOf } from "@/components/board/model";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { ChangedFiles } from "@/components/item/Extras";
import { asStamp } from "@/components/item/stamps";
import { isItemView, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { Markdown } from "@/components/markdown/Markdown";
import { hrefFor, taskHref } from "@/components/markdown/links";
import { useGo, useItemRows, useResolver } from "@/components/markdown/useResolver";
import { textField } from "@/components/planning/progress";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Loading } from "@/components/ui/skeleton";
import { CommitRows } from "@/components/work/CommitRows";
import { Decision, StatusPicker } from "@/components/work/OwnerActions";
import { WorkRef } from "@/components/work/TaskPage";
import { isOver, readTask, useTaskCommits } from "@/components/work/taskModel";

// The board is drawn at 14px; the dialog sits in a portal, so it sets its own size instead of inheriting the page's 16px.
const BOX =
  "top-0 left-0 h-dvh max-h-none w-screen max-w-none translate-x-0 overflow-y-auto rounded-none border-0 text-sm leading-[1.5] " +
  "md:top-10 md:left-1/2 md:h-auto md:max-h-[calc(100dvh-5rem)] md:w-[calc(100vw-2rem)] md:max-w-[1040px] md:-translate-x-1/2 md:rounded-xl";

// The board sets one line height and no paragraph margins; the section's own gap spaces the text.
const TEXT = "max-w-[62ch] [&>div]:leading-[1.5] [&_p]:my-0 [&_p+p]:mt-2";
const SECTION = "flex flex-col gap-1.5";
const HEADING = "m-0 text-[15px] font-semibold";
const LINK = "font-medium text-primary hover:text-foreground";

function Section({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className={SECTION}>
      <h2 className={`${HEADING} flex justify-between`}>
        {title}
        {aside}
      </h2>
      {children}
    </section>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-xs leading-[1.5] text-muted-foreground">{label}</dt>
      <dd className="m-0 font-medium">{children}</dd>
    </div>
  );
}

function capitalised(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** What the commit request costs is paid only once the reader opens the list. */
function Changes({ item }: { item: ItemView }) {
  const commits = useTaskCommits(item.id);
  const changed = item.changed_files ?? [];
  return (
    <div className="mt-3 flex flex-col gap-4">
      {changed.length === 0 ? null : <ChangedFiles files={changed} />}
      <Answer query={commits} label="Reading the commits...">
        {(list) => (list.length === 0 ? <Empty>No commit names this task.</Empty> : <CommitRows commits={list} />)}
      </Answer>
    </div>
  );
}

function Content({ item }: { item: ItemView }) {
  const resolver = useResolver();
  const go = useGo();
  const [opened, setOpened] = useState(false);
  const { goal } = readTask(item.body);
  // The engine reads both, so the card and this view count the same boxes.
  const criteria = item.criteria ?? [];
  const outcome = item.outcome ?? null;
  const ticked = criteria.filter((check) => check.checked).length;
  return (
    <>
      {goal === null ? null : (
        <Section title="What this is">
          <div className={TEXT}>
            <Markdown source={goal} resolver={resolver} fromId={item.id} onNavigate={go} compact />
          </div>
        </Section>
      )}
      {criteria.length === 0 ? null : (
        <Section
          title="Done when"
          aside={<span className={`font-mono text-xs leading-[1.5] font-medium ${ticked === criteria.length ? "text-st-done" : "text-muted-foreground"}`}>{`${ticked} of ${criteria.length}`}</span>}
        >
          <ul className="m-0 list-none p-0">
            {criteria.map((check) => (
              <li key={check.n}>
                <label className="flex gap-[9px] py-[3px]">
                  <input type="checkbox" checked={check.checked} disabled readOnly className="mt-[5px] shrink-0" />
                  {check.text.replace(/\*\*|`/g, "")}
                </label>
              </li>
            ))}
          </ul>
        </Section>
      )}
      {outcome === null ? null : (
        <Section title="Outcome from the agent">
          <div className={`${TEXT} rounded-lg bg-background px-3 py-2.5`}>
            <Markdown source={outcome} resolver={resolver} fromId={item.id} onNavigate={go} compact />
          </div>
        </Section>
      )}
      <details className="border-t pt-2.5" onToggle={(event) => event.currentTarget.open && setOpened(true)}>
        <summary className="cursor-pointer text-muted-foreground">Changed files and commits</summary>
        {opened ? <Changes item={item} /> : null}
      </details>
    </>
  );
}

function Facts({ row, item, onDone }: { row: ListRow; item: ItemView | null; onDone: () => void }) {
  const resolver = useResolver();
  const rows = useItemRows();
  const byId = useMemo(() => new Map((rows.data ?? []).map((entry) => [entry.id, entry])), [rows.data]);
  const waiting = (row.depends_on ?? []).flatMap((target) => {
    const id = resolver.wiki(target)?.id ?? target;
    const found = byId.get(id);
    return isOver(found?.status) ? [] : [{ id, short: found?.short ?? id.replace(/^tasks\//, ""), title: found?.title ?? id }];
  });
  const written = item === null ? null : asStamp(item.fields.generated);
  return (
    <aside aria-label="Facts" className="grid flex-[1_1_260px] content-start gap-3.5 bg-background px-6 py-5 md:rounded-br-xl">
      <dl className="m-0 grid gap-3.5">
        {item === null ? null : (
          <Fact label="Status">
            <StatusPicker item={item} onDone={onDone} />
          </Fact>
        )}
        {row.priority === null ? null : <Fact label="Priority">{capitalised(row.priority)}</Fact>}
        {row.milestone === undefined ? null : (
          <Fact label="Milestone">
            <WorkRef value={row.milestone} store="milestones" resolver={resolver} empty="" className={LINK} />
          </Fact>
        )}
        {row.plan === undefined ? null : (
          <Fact label="Plan">
            <WorkRef value={row.plan} store="plans" resolver={resolver} empty="" className={LINK} />
          </Fact>
        )}
        {(row.services ?? []).length === 0 ? null : <Fact label="Service">{(row.services ?? []).join(", ")}</Fact>}
        {row.assignee === undefined ? null : <Fact label="Owner">{row.assignee}</Fact>}
        {waiting.length === 0 ? null : (
          <Fact label="Waits on">
            {waiting.map((task, at) => (
              <span key={task.id}>
                {at === 0 ? "" : ", "}
                <AppLink href={taskHref(task.id)} title={task.title} className={LINK}>
                  {task.short}
                </AppLink>
              </span>
            ))}
          </Fact>
        )}
        {written === null ? null : <Fact label="Written by">{`${written.by}, ${written.at}`}</Fact>}
      </dl>
    </aside>
  );
}

/** A task over the page it was opened from: content on the left, facts on the right, empty rows left out. */
export default function TaskOverlay({ row, onClose }: { row: ListRow; onClose: () => void }) {
  const query = useItemQuery(row.id);
  const [today] = useState(() => new Date());
  const view = query.data && isItemView(query.data) ? query.data : null;
  const meta = [row.short ?? row.id, row.status === null ? null : columnLabel(row.status), ageOf(row.created, today)].filter((part) => part !== null && part !== "").join(" · ");
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className={BOX}>
        <header className="flex flex-wrap items-center gap-x-3.5 gap-y-2.5 border-b px-6 py-[18px]">
          <div className="min-w-0 flex-[1_1_400px]">
            <p className="m-0 mb-0.5 font-mono text-xs leading-[1.5] text-muted-foreground">{meta}</p>
            <DialogTitle className="m-0 text-[21px] leading-[1.25] font-[650] tracking-[-0.015em]">{row.title ?? row.id}</DialogTitle>
          </div>
          {view === null ? null : <Decision task={{ id: view.id, etag: view.etag, status: textField(view.fields, "status") }} onDone={onClose} />}
          <AppLink href={hrefFor(row.id)} className={`px-2.5 py-2 ${LINK}`}>
            Open full page
          </AppLink>
          <button type="button" onClick={onClose} className="rounded-md px-2.5 py-2 text-muted-foreground hover:text-foreground">
            Close
          </button>
        </header>
        <div className="flex flex-wrap">
          <div className="flex min-w-0 flex-[999_1_460px] flex-col gap-5 px-6 pt-5 pb-6">
            {query.isPending ? <Loading lines={7} /> : null}
            {query.isError ? <ErrorNotice error={query.error} /> : null}
            {view === null ? null : <Content item={view} />}
          </div>
          <Facts row={row} item={view} onDone={onClose} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
