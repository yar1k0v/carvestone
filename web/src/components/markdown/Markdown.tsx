import { type ComponentProps, type ReactNode, isValidElement, useEffect, useMemo } from "react";
import ReactMarkdown, { type Components, type Options, defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";
import { C4View } from "@/components/c4/C4View";
import { isC4 } from "@/components/c4/parse";
import { Mermaid } from "./Mermaid";
import { codeHref, type Resolved, type Resolver } from "./links";
import { internalClick } from "./nav";
import { CODE_SCHEME, WIKI_SCHEME, headingIds, wikilinks } from "./remark";

export interface MarkdownProps {
  source: string;
  resolver: Resolver;
  /** The id of the item the text belongs to; relative `.md` links start from it. */
  fromId?: string;
  /** Sends an in-app link to the router. Without it links are plain anchors. */
  onNavigate?: (href: string) => void;
  /** Heading anchor to scroll to once the text is on screen. */
  activeAnchor?: string;
  /** Smaller text for a side panel; the default is set for reading. */
  compact?: boolean;
}

const HEADING_CLASS: Record<string, string> = {
  h1: "mt-10 mb-4 text-[1.75em] leading-tight font-bold tracking-[-0.02em] first:mt-0",
  h2: "mt-9 mb-3 text-[1.375em] leading-[1.36] font-semibold tracking-[-0.01em] first:mt-0",
  h3: "mt-7 mb-2 text-[1.125em] leading-snug font-semibold",
  h4: "mt-6 mb-1.5 font-semibold",
  h5: "mt-5 mb-1 text-[0.9em] font-semibold",
  h6: "mt-5 mb-1 text-[0.9em] font-semibold text-muted-foreground",
};

function heading(Tag: "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
  return function Heading({ id, children }: ComponentProps<"h1">) {
    return (
      <Tag id={id} className={`group scroll-mt-6 ${HEADING_CLASS[Tag]}`}>
        {children}
        {id ? (
          <a
            href={`#${id}`}
            aria-label="Link to this section"
            className="ml-2 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus:opacity-100"
          >
            #
          </a>
        ) : null}
      </Tag>
    );
  };
}

function isMermaid(className: string | undefined): boolean {
  return /(?:^|\s)language-mermaid(?:\s|$)/.test(className ?? "");
}

function text(children: ReactNode): string {
  return typeof children === "string" ? children : Array.isArray(children) ? children.map(text).join("") : "";
}

const STATIC: Components = {
  h1: heading("h1"),
  h2: heading("h2"),
  h3: heading("h3"),
  h4: heading("h4"),
  h5: heading("h5"),
  h6: heading("h6"),
  p: ({ children }) => <p className="my-[0.9em]">{children}</p>,
  ul: ({ children, className }) => (
    <ul className={`my-[0.9em] list-disc pl-6 marker:text-muted-foreground ${className?.includes("contains-task-list") ? "list-none pl-1" : ""}`}>
      {children}
    </ul>
  ),
  ol: ({ children }) => <ol className="my-[0.9em] list-decimal pl-6 marker:text-muted-foreground">{children}</ol>,
  li: ({ children }) => <li className="my-[0.35em] pl-1">{children}</li>,
  blockquote: ({ children }) => (
    <blockquote className="my-[1.1em] border-l-2 pl-4 text-muted-foreground">{children}</blockquote>
  ),
  hr: () => <hr className="my-10" />,
  table: ({ children }) => (
    <div className="slab my-6 overflow-x-auto">
      <table className="w-full border-collapse text-[0.9375rem] leading-[1.375rem] [&_tr:last-child>td]:border-b-0">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b bg-sidebar px-3.5 py-2 text-left font-medium whitespace-nowrap text-muted-foreground">{children}</th>,
  td: ({ children }) => <td className="border-b border-muted px-3.5 py-2 align-top">{children}</td>,
  // A fence tagged `mermaid` becomes a diagram; any other fence stays code.
  pre: ({ children }) => {
    if (isValidElement<{ className?: string }>(children) && isMermaid(children.props.className)) return <>{children}</>;
    return (
      <pre className="my-5 overflow-x-auto rounded-lg border bg-sidebar p-3.5 font-mono text-[0.8125rem] leading-5 [&>code]:rounded-none [&>code]:bg-transparent [&>code]:p-0">
        {children}
      </pre>
    );
  },
  // Nothing may load from another origin, so an external image shows its alt text instead.
  img: ({ src, alt }) =>
    typeof src === "string" && !/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(src) ? (
      <img src={src} alt={alt ?? ""} className="my-3 max-w-full" />
    ) : (
      <span className="text-muted-foreground">[image: {alt ?? "external"}]</span>
    ),
};

function urlTransform(url: string): string {
  return url.startsWith(WIKI_SCHEME) || url.startsWith(CODE_SCHEME) ? url : defaultUrlTransform(url);
}

const LINK_CLASS = "text-primary underline underline-offset-2 hover:no-underline";

function Broken({ children, why }: { children: ReactNode; why: string }) {
  return (
    <span data-broken="true" title={why} className="text-destructive underline decoration-dotted underline-offset-2">
      {children}
    </span>
  );
}

/** A Mermaid fence that starts with `C4...` is drawn by stone's own C4 viewer; other Mermaid by Mermaid. */
function makeCode({ resolver, onNavigate }: Pick<MarkdownProps, "resolver" | "onNavigate">) {
  return function Code({ className, children }: ComponentProps<"code">) {
    if (isMermaid(className)) {
      const chart = text(children).replace(/\n$/, "");
      return isC4(chart) ? <C4View source={chart} resolver={resolver} onNavigate={onNavigate} /> : <Mermaid chart={chart} />;
    }
    return <code className={className ?? "rounded bg-muted px-1 py-0.5 font-mono text-[0.85em] [overflow-wrap:anywhere]"}>{children}</code>;
  };
}

/** A `[[docs/x]]` reads as the doc's title, not its id; a numbered item keeps its short id. */
function wikiLabel(found: Resolved, target: string, children: ReactNode): ReactNode {
  const named = /^(?:docs|arch|plans|refs)\//.test(found.id);
  return named && found.title !== null && found.title !== found.id && text(children) === target ? found.title : children;
}

function makeLink({ resolver, fromId, onNavigate }: Pick<MarkdownProps, "resolver" | "fromId" | "onNavigate">) {
  return function Link({ href, children }: ComponentProps<"a">) {
    if (href === undefined || href === "") return <span>{children}</span>;
    const internal = (to: string, label: ReactNode = children) => (
      <a href={to} className={LINK_CLASS} onClick={internalClick(onNavigate, to)}>
        {label}
      </a>
    );
    if (href.startsWith(CODE_SCHEME)) {
      const target = decodeURIComponent(href.slice(CODE_SCHEME.length));
      const to = codeHref(target);
      return (
        <a
          href={to}
          data-code-chip="true"
          title={`Open ${target} in the code explorer`}
          className="rounded border bg-muted px-1.5 py-0.5 font-mono text-[0.85em] hover:bg-accent"
          onClick={internalClick(onNavigate, to)}
        >
          {children}
        </a>
      );
    }
    if (href.startsWith(WIKI_SCHEME)) {
      const target = decodeURIComponent(href.slice(WIKI_SCHEME.length));
      const found = resolver.wiki(target);
      if (found) return internal(found.href, wikiLabel(found, target, children));
      return resolver.ready ? <Broken why={`${target} was not found`}>{children}</Broken> : <span>{children}</span>;
    }
    if (href.startsWith("#")) {
      return (
        <a href={href} className={LINK_CLASS}>
          {children}
        </a>
      );
    }
    if (/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(href)) {
      return (
        <a href={href} target="_blank" rel="noopener noreferrer" className={LINK_CLASS}>
          {children}
        </a>
      );
    }
    const path = href.split("#")[0] ?? "";
    if (path.endsWith(".md")) {
      const found = resolver.md(href, fromId ?? null);
      if (found) return internal(found.href);
      return resolver.ready ? <Broken why={`${href} was not found`}>{children}</Broken> : <span>{children}</span>;
    }
    return (
      <a href={href} className={LINK_CLASS}>
        {children}
      </a>
    );
  };
}

/** GitHub-flavoured markdown with the engine's heading anchors, wikilinks, code chips and Mermaid. */
export function Markdown({ source, resolver, fromId, onNavigate, activeAnchor, compact }: MarkdownProps) {
  const plugins = useMemo<NonNullable<Options["remarkPlugins"]>>(
    () => [remarkGfm, wikilinks, headingIds(source)],
    [source],
  );
  const components = useMemo<Components>(
    () => ({ ...STATIC, a: makeLink({ resolver, fromId, onNavigate }), code: makeCode({ resolver, onNavigate }) }),
    [resolver, fromId, onNavigate],
  );

  useEffect(() => {
    if (!activeAnchor) return;
    const element = document.getElementById(activeAnchor);
    if (element && typeof element.scrollIntoView === "function") element.scrollIntoView();
  }, [activeAnchor, source]);

  return (
    <div className={compact ? "text-sm leading-6 [overflow-wrap:anywhere]" : "text-base leading-[1.625] [overflow-wrap:anywhere]"}>
      <ReactMarkdown remarkPlugins={plugins} components={components} urlTransform={urlTransform}>
        {source}
      </ReactMarkdown>
    </div>
  );
}
