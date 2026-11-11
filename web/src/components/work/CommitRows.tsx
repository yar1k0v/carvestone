import { useState } from "react";
import type { FeatureCommit } from "@/api/types/FeatureCommit";
import type { LinkHow } from "@/api/types/LinkHow";
import type { TaskCommit } from "@/api/types/TaskCommit";
import { Tag } from "@/components/architecture/service/parts";
import { ago } from "@/components/docs/library";
import { More } from "@/components/ui/page";

const SHOWN = 15;

const HOW: Record<LinkHow, { label: string; hint: string }> = {
  refs: { label: "Refs line", hint: "The commit names the task in a Refs line." },
  message: { label: "Named in message", hint: "The commit message names the task." },
  outcome: { label: "Named in outcome", hint: "The task's outcome names this commit." },
  path: { label: "Same files", hint: "Nothing names the two. The commit changed files only this task works on, after the task was created." },
};

/** Commits, each saying how it was tied to its task; a feature's also say when that task is only related to it. */
export function CommitRows({ commits }: { commits: (TaskCommit | FeatureCommit)[] }) {
  const [now] = useState(() => new Date());
  return (
    <More rows={commits} limit={SHOWN}>
      {(shown) => (
        <ul aria-label="Commits" className="slab divide-y overflow-hidden">
          {shown.map((commit) => (
            <li key={`${commit.repo}:${commit.sha}`} className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 px-3.5 py-2 text-sm">
              <code title={commit.sha} className="shrink-0 font-mono text-xs text-muted-foreground">
                {commit.sha.slice(0, 7)}
              </code>
              <span className="min-w-0 flex-1 basis-60">{commit.title}</span>
              {commit.repo === "" ? null : <Tag>{commit.repo}</Tag>}
              <Tag title={HOW[commit.how].hint}>{HOW[commit.how].label}</Tag>
              {"inferred" in commit && commit.inferred ? <Tag title={`From ${commit.task}, which nothing links to this feature. They only share something.`}>Related</Tag> : null}
              <span title={commit.date} className="shrink-0 text-xs text-muted-foreground">
                {ago(commit.date.slice(0, 10), now)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </More>
  );
}
