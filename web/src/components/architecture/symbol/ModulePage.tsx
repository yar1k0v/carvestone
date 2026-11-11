import type { Dependency } from "@/api/types/Dependency";
import type { Explain } from "@/api/types/Explain";
import type { Module } from "@/api/types/Module";
import { Notes } from "@/components/graph/CodeParts";
import { serviceHref } from "@/components/graph/serviceHref";
import { type TitleOf, titlesOf } from "@/components/architecture/service/model";
import { useArchGraph } from "@/components/architecture/service/useService";
import { useCode } from "@/components/graph/useCode";
import { useHld } from "@/components/graph/useDesign";
import { ErrorNotice } from "@/components/item/ErrorNotice";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { type Crumb, useCrumbs, useOutline } from "@/shell/pageFrame";
import { classHref, moduleHref } from "./href";
import { OUTLINE, entryTarget, folderName, kindWord, moduleName, nameOf, splitKeySymbol } from "./model";
import { LINK, Muted, RelatedList, Row, RowList, Section } from "./parts";
import { useElementTitle, useModules, useWhy } from "./useSymbol";

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

function peerHref(component: string): string | null {
  if (component.startsWith("mod:")) return moduleHref(component);
  return component.startsWith("(") ? null : serviceHref(component);
}

/** Other modules on one side of this one, the busiest first. */
function Peers({ peers, modules, titleOf, empty }: { peers: Dependency[]; modules: Module[]; titleOf: TitleOf; empty: string }) {
  if (peers.length === 0) return <Muted>{empty}</Muted>;
  const rows = [...peers].sort((a, b) => b.exact + b.inferred - (a.exact + a.inferred));
  return (
    <RowList rows={rows}>
      {(peer) => {
        const known = modules.find((module) => module.id === peer.component);
        const href = peerHref(peer.component);
        const service = peer.component.startsWith("arch/") ? titleOf(peer.component) : null;
        const label = known?.label || service || (peer.component === "(unowned)" ? "Code outside any module" : peer.component);
        return (
          <Row key={peer.component}>
            <div className="flex flex-wrap items-baseline justify-between gap-x-2">
              {href === null ? (
                <span>{label}</span>
              ) : (
                <AppLink href={href} className={`break-all font-mono ${LINK}`}>
                  {label}
                </AppLink>
              )}
              <span className="text-xs text-muted-foreground">
                {plural(peer.exact + peer.inferred, "link")}
                {peer.inferred > 0 ? `, ${peer.inferred} inferred` : ""}
              </span>
            </div>
          </Row>
        );
      }}
    </RowList>
  );
}

function Sections({ id, explain, modules }: { id: string; explain: Explain; modules: Module[] }) {
  const module = modules.find((candidate) => candidate.id === id);
  const why = useWhy(explain.files[0]?.path);
  const service = why.data?.component ?? null;
  const serviceTitle = useElementTitle(service);
  const hld = useHld();
  const graph = useArchGraph();
  const titleOf = titlesOf(hld.data, graph.data);
  const name = module === undefined ? folderName(id) : moduleName(module);
  const fileCount = module?.files.length ?? (explain.folders ?? []).reduce((sum, row) => sum + row.files, 0);
  const total = Math.max(fileCount, explain.files.length);

  const crumbs: Crumb[] = [{ label: "System", href: "/architecture" }];
  if (service !== null && serviceTitle !== null) crumbs.push({ label: serviceTitle, href: serviceHref(service) });
  crumbs.push({ label: name });
  useCrumbs(crumbs);
  useOutline(OUTLINE);

  const related = [
    ...explain.docs.map((doc) => ({ id: doc, href: hrefFor(doc), label: doc, how: "written about this code" })),
    ...explain.tasks.map((task) => ({ id: task, href: hrefFor(task), label: task, how: "works on this code" })),
  ];

  return (
    <article>
      <PageHeader
        eyebrow={id.startsWith("mod:") ? "Module" : "Folder"}
        title={<span className="break-all">{name}</span>}
        description={<span className="break-all font-mono text-sm">{module?.label ?? id}</span>}
      />
      <Section id="purpose" title="Purpose">
        <p className="text-sm">
          {plural(total, "file")}
          {module === undefined || module.project === "-" ? null : ` in the project ${module.project}`}
          {service === null ? null : (
            <>
              {", part of "}
              <AppLink href={serviceHref(service)} className={LINK}>
                {serviceTitle}
              </AppLink>
            </>
          )}
          .
        </p>
        {explain.uses.length > 0 ? (
          <ul className="mt-3 list-disc space-y-1 pl-5 text-sm">
            {explain.uses.map((use) => (
              <li key={use.target}>{use.description}</li>
            ))}
          </ul>
        ) : null}
        {explain.entries.length > 0 ? (
          <>
            <h3 className="mt-4 mb-1 text-sm font-medium">Where execution starts</h3>
            <ul className="space-y-1 text-sm">
              {explain.entries.map((entry) => (
                <li key={`${entry.kind}:${entry.target}`} className="flex flex-wrap gap-x-2">
                  <AppLink href={classHref(entryTarget(entry.target))} className={`break-all font-mono ${LINK}`}>
                    {entry.detail || nameOf(entryTarget(entry.target))}
                  </AppLink>
                  <span className="text-muted-foreground">{entry.kind}</span>
                </li>
              ))}
            </ul>
          </>
        ) : null}
        <RelatedList rows={related} empty="No document or task is tied to this module yet." />
      </Section>
      <Section id="members" title="Members">
        <Notes note={explain.note} truncated={explain.truncated} />
        {explain.key_symbols.length === 0 ? (
          <Muted>The index found no symbols here.</Muted>
        ) : (
          <>
            <p className="mb-2 text-sm text-muted-foreground">The symbols other code reaches most.</p>
            <RowList rows={explain.key_symbols}>
              {(hit) => {
                const { incoming, signature } = splitKeySymbol(hit.snippet);
                return (
                  <Row key={`${hit.id}:${hit.lines[0]}`}>
                    <div className="flex flex-wrap items-baseline gap-x-2">
                      <AppLink href={classHref(hit.id)} className={`font-mono ${LINK}`}>
                        {nameOf(hit.id)}
                      </AppLink>
                      <span className="text-xs text-muted-foreground">
                        {kindWord(hit.kind, signature).toLowerCase()}
                        {incoming === null ? "" : `, ${plural(incoming, "incoming link")}`}
                      </span>
                    </div>
                    <p className="truncate font-mono text-xs text-muted-foreground">{signature}</p>
                  </Row>
                );
              }}
            </RowList>
          </>
        )}
      </Section>
      <Section id="used-by" title="Used by">
        <Peers peers={explain.inbound} modules={modules} titleOf={titleOf} empty="No other module uses this one." />
      </Section>
      <Section id="uses" title="Uses">
        <Peers peers={explain.outbound} modules={modules} titleOf={titleOf} empty="It uses no other module." />
      </Section>
      <Section id="source" title="Source">
        {explain.files.length === 0 ? (
          <Muted>The index holds no files for this module.</Muted>
        ) : (
          <>
          <RowList rows={explain.files} limit={20}>
            {(file) => (
              <Row key={file.path}>
                <div className="flex flex-wrap items-baseline justify-between gap-x-2">
                  <AppLink href={classHref(file.path)} className={`break-all font-mono ${LINK}`}>
                    {file.path}
                  </AppLink>
                  <span className="text-xs text-muted-foreground">
                    {file.snippet}, {plural(file.lines[1], "line")}
                  </span>
                </div>
              </Row>
            )}
          </RowList>
          {total > explain.files.length ? (
            <p className="mt-2 text-xs text-muted-foreground">
              Showing {explain.files.length} of {total} files.
            </p>
          ) : null}
          </>
        )}
      </Section>
    </article>
  );
}

/** A module the code index found, or a folder: purpose, members, used by, uses, source. */
export function ModulePage({ id }: { id: string }) {
  const explain = useCode<Explain>("explain", { target: id });
  const modules = useModules();
  if (explain.isPending || modules.isPending) return <Loading lines={6} />;
  if (explain.isError) {
    return (
      <>
        <PageHeader title={<span className="break-all">{id}</span>} />
        <ErrorNotice error={explain.error} />
      </>
    );
  }
  return <Sections id={id} explain={explain.data} modules={modules.data ?? []} />;
}
