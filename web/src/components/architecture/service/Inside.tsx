import { useState } from "react";
import type { Explain } from "@/api/types/Explain";
import { moduleHref } from "@/components/architecture/symbol/href";
import { useModules } from "@/components/architecture/symbol/useSymbol";
import { AppLink } from "@/components/markdown/AppLink";
import { More } from "@/components/ui/page";
import { cn } from "@/lib/utils";
import { type ModuleRow, linksText, moduleGroups, modulesIn } from "./model";
import { Empty, link } from "./parts";

function files(n: number): string {
  return `${n} ${n === 1 ? "file" : "files"}`;
}

/** A folder holds one module, several, or none the index grouped; each case opens what exists. */
function Open({ row, folders }: { row: ModuleRow; folders: string[] }) {
  const modules = useModules();
  const found = modulesIn(modules.data ?? [], row.folder, folders);
  const [only] = found;
  if (only === undefined) {
    return (
      <AppLink href={moduleHref(row.folder)} className={link}>
        Open folder
      </AppLink>
    );
  }
  if (found.length === 1) {
    return (
      <AppLink href={moduleHref(only.id)} className={link}>
        Open module
      </AppLink>
    );
  }
  return (
    <span className="basis-full">
      <More rows={found} limit={8}>
        {(shown) => (
          <span className="flex flex-wrap gap-x-3">
            {shown.map((module) => (
              <AppLink key={module.id} href={moduleHref(module.id)} className={link}>
                {module.label.startsWith(`${row.folder}/`) ? module.label.slice(row.folder.length + 1) : `${row.name}, ${files(module.files.length)}`}
              </AppLink>
            ))}
          </span>
        )}
      </More>
    </span>
  );
}

function Chosen({ module, group, folders }: { module: ModuleRow; group: string; folders: string[] }) {
  return (
    <aside aria-label="Selected module" className="slab grid min-w-0 flex-[1_1_280px] gap-2.5 px-[18px] py-4">
      <h3 className="text-base font-semibold">{module.name}</h3>
      <p className="text-muted-foreground">
        {group}, {files(module.files)}
      </p>
      <div>
        <p className="text-xs text-muted-foreground">Calls</p>
        <p>{linksText(module.calls)}</p>
      </div>
      <div>
        <p className="text-xs text-muted-foreground">Called by</p>
        <p>{linksText(module.calledBy)}</p>
      </div>
      <Open row={module} folders={folders} />
    </aside>
  );
}

/** The modules of a service in groups; choosing one says what it calls and what calls it. */
export function Inside({ explain, title }: { explain: Explain; title: string }) {
  const groups = moduleGroups(explain, title);
  const all = groups.flatMap((group) => group.modules);
  const [picked, setPicked] = useState<string | null>(null);
  const chosen = all.find((module) => module.folder === picked) ?? all[0];
  if (chosen === undefined) return <Empty>The code index found no folders inside this service.</Empty>;
  const chosenGroup = groups.find((group) => group.modules.includes(chosen))?.title ?? "";
  return (
    <div className="flex flex-wrap items-start gap-5">
      <div className="slab min-w-0 flex-[2_1_460px]">
        {groups.map((group) => (
          <div key={group.title}>
            <p className="px-3.5 pt-2.5 pb-1 text-xs text-muted-foreground">{group.title}</p>
            {group.modules.map((module) => (
              <button
                key={module.folder}
                type="button"
                aria-pressed={module.folder === chosen.folder}
                onClick={() => setPicked(module.folder)}
                className={cn(
                  "grid w-full grid-cols-[minmax(0,1fr)_auto] gap-3 border-b px-3.5 py-[9px] text-left last:border-b-0 hover:bg-primary/5",
                  module.folder === chosen.folder && "bg-primary/10",
                )}
              >
                <span className="font-medium">{module.name}</span>
                <span className="font-mono text-xs text-muted-foreground">{files(module.files)}</span>
              </button>
            ))}
          </div>
        ))}
      </div>
      <Chosen module={chosen} group={chosenGroup} folders={all.map((module) => module.folder)} />
    </div>
  );
}
