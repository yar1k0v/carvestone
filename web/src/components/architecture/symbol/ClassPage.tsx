import { useRouter } from "@tanstack/react-router";
import type { Backlink } from "@/api/types/Backlink";
import type { SymbolPage } from "@/api/types/SymbolPage";
import type { Why } from "@/api/types/Why";
import { plainNote } from "@/components/architecture/service/model";
import { CodeError } from "@/components/graph/CodeParts";
import { serviceHref } from "@/components/graph/serviceHref";
import { isCodeView, useItemQuery } from "@/components/item/useItem";
import { AppLink } from "@/components/markdown/AppLink";
import { hrefFor } from "@/components/markdown/links";
import { PageHeader } from "@/components/ui/page";
import { Loading } from "@/components/ui/skeleton";
import { type Crumb, useCrumbs, useOutline } from "@/shell/pageFrame";
import { classHref, moduleHref } from "./href";
import { OUTLINE, kindWord, moduleName, moduleOf, nameOf, ownerOf, uniqueMembers, withArticle } from "./model";
import { EdgeList, LINK, MemberList, Muted, type Related, RelatedList, Section, SourceText } from "./parts";
import { useElementTitle, useModules, useSymbol, useWhy } from "./useSymbol";

const VIA: Record<string, string> = {
  trailer: "named by a commit that changed it",
  task: "lists this code",
  component: "written for its service",
  path: "lists this file",
  "code link": "links to this code",
};

function relatedOf(backlinks: Backlink[], why: Why | undefined): Related[] {
  const rows = new Map<string, Related>();
  for (const link of backlinks) {
    rows.set(link.id, { id: link.id, href: hrefFor(link.id, link.anchor), label: link.id, how: "links to this code" });
  }
  for (const item of [...(why?.docs ?? []), ...(why?.decisions ?? []), ...(why?.plans ?? []), ...(why?.tasks ?? [])]) {
    if (rows.has(item.id)) continue;
    rows.set(item.id, { id: item.id, href: hrefFor(item.id), label: item.title ?? item.id, how: VIA[item.via] ?? item.via });
  }
  return [...rows.values()];
}

function Sections({ page }: { page: SymbolPage }) {
  const { id, kind, signature, source } = page;
  const isFile = kind === "file";
  const item = useItemQuery(id);
  const backlinks = item.data !== undefined && isCodeView(item.data) ? item.data.backlinks : [];
  const path = source?.file;
  const why = useWhy(path, isFile ? undefined : source?.start_line);
  const modules = useModules();
  const module = path === undefined ? null : moduleOf(modules.data ?? [], path);
  const service = why.data?.component ?? null;
  const serviceTitle = useElementTitle(service);
  const owner = kind === "method" || kind === "member" ? ownerOf(id) : null;
  const name = isFile ? nameOf(id, true) : page.name;
  const word = kindWord(kind, signature);

  const crumbs: Crumb[] = [{ label: "System", href: "/architecture" }];
  if (service !== null && serviceTitle !== null) crumbs.push({ label: serviceTitle, href: serviceHref(service) });
  if (module !== null) crumbs.push({ label: moduleName(module), href: moduleHref(module.id) });
  if (owner !== null) crumbs.push({ label: nameOf(owner), href: classHref(owner) });
  crumbs.push({ label: name });
  useCrumbs(crumbs);
  useOutline(OUTLINE);

  return (
    <article>
      <PageHeader
        eyebrow={word}
        title={<span className="break-all">{name}</span>}
        description={<span className="break-all font-mono text-sm">{id}</span>}
      />
      <Section id="purpose" title="Purpose">
        <p className="text-sm">
          {withArticle(word)}
          {owner === null ? null : (
            <>
              {" of "}
              <AppLink href={classHref(owner)} className={`font-mono ${LINK}`}>
                {nameOf(owner)}
              </AppLink>
            </>
          )}
          {module === null ? null : (
            <>
              {" in the module "}
              <AppLink href={moduleHref(module.id)} className={LINK}>
                {moduleName(module)}
              </AppLink>
            </>
          )}
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
        {page.doc === undefined ? null : <p className="mt-3 text-sm whitespace-pre-line">{page.doc}</p>}
        {signature === "" ? null : (
          <pre className="mt-3 overflow-x-auto rounded-lg border bg-muted/50 p-3 font-mono text-xs">{signature}</pre>
        )}
        <RelatedList rows={relatedOf(backlinks, why.data)} empty="No document or task is tied to this code yet." />
      </Section>
      <Section id="members" title="Members">
        {page.note === undefined ? null : <p className="mb-2 text-xs text-muted-foreground">{plainNote(page.note)}</p>}
        <MemberList members={uniqueMembers(page.members)} />
      </Section>
      <Section id="used-by" title="Used by">
        <EdgeList edges={page.used_by} empty="Nothing in the code uses this." />
      </Section>
      <Section id="uses" title="Uses">
        <EdgeList edges={page.uses} empty="It uses nothing the code index knows." />
      </Section>
      <Section id="source" title="Source">
        {source === undefined ? (
          <Muted>{page.source_private ? "Not shown: the index has no language for this file." : "The file could not be read."}</Muted>
        ) : (
          <>
            <p className="mb-2 font-mono text-sm break-all">
              {isFile ? (
                source.file
              ) : (
                <>
                  <AppLink href={classHref(source.file)} className={LINK}>
                    {source.file}
                  </AppLink>
                  :{source.start_line}-{source.end_line}
                </>
              )}
            </p>
            <SourceText source={source} />
          </>
        )}
        {(why.data?.commits.length ?? 0) > 0 ? (
          <ul className="mt-3 space-y-1 text-sm">
            {why.data?.commits.slice(0, 5).map((commit) => (
              <li key={commit.sha || commit.subject} className="flex gap-2">
                <span className="shrink-0 font-mono text-xs leading-5 text-muted-foreground">
                  {commit.uncommitted ? "not committed" : commit.sha.slice(0, 7)}
                </span>
                <span className="min-w-0 flex-1">{commit.subject}</span>
                <span className="shrink-0 text-xs leading-5 text-muted-foreground">{commit.date.slice(0, 10)}</span>
              </li>
            ))}
          </ul>
        ) : null}
      </Section>
    </article>
  );
}

/** A class, method or file: purpose, members, used by, uses, source. `target` is any name the engine takes. */
export function ClassPage({ target }: { target: string }) {
  const router = useRouter();
  const query = useSymbol(target);
  if (query.isPending) return <Loading lines={6} />;
  if (query.isError) {
    return (
      <>
        <PageHeader title={<span className="break-all">{target}</span>} />
        <CodeError error={query.error} onPick={(option) => router.history.push(classHref(option))} />
      </>
    );
  }
  return <Sections key={query.data.id} page={query.data} />;
}
