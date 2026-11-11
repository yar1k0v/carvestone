import { Search } from "lucide-react";
import { type KeyboardEvent, useEffect, useState } from "react";
import { AppLink } from "@/components/markdown/AppLink";
import { useGo } from "@/components/markdown/useResolver";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { hitHref } from "./hits";
import { SearchHits } from "./SearchHits";
import { useGroupedHits, useSearch } from "./useSearch";

/** `/` is a shortcut only when the page is not taking text. */
function typingInto(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

function Palette({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const go = useGo();
  const { result, query, busy, error } = useSearch(text);
  const groups = useGroupedHits(result?.hits);
  // Arrow keys and Enter follow the order the rows show in, not the order of the answer.
  const hits = groups.flatMap((group) => group.hits);

  const open = (href: string) => {
    onClose();
    go(href);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((at) => Math.min(Math.max(at + step, 0), Math.max(hits.length - 1, 0)));
    } else if (event.key === "Enter" && !busy) {
      event.preventDefault();
      const shown = hits[active];
      if (shown) open(hitHref(shown.hit));
      else if (text.trim() !== "") open(`/search?q=${encodeURIComponent(text.trim())}`);
    }
  };

  return (
    <div onKeyDown={onKeyDown}>
      <div className="flex items-center gap-2 border-b px-3">
        <Search className="size-4 text-muted-foreground" />
        <input
          aria-label="Search items and code"
          placeholder="Search items and code"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            setActive(0);
          }}
          className="h-11 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
      <div className="max-h-[50vh] overflow-auto p-1" onClick={onClose}>
        {error ? <p className="px-3 py-4 text-sm text-destructive">{error.message}</p> : null}
        {result ? <SearchHits groups={groups} query={query} active={active} compact /> : null}
        {!result && !error && text.trim() !== "" ? <p className="px-3 py-6 text-center text-sm text-muted-foreground">Searching...</p> : null}
      </div>
      {text.trim() !== "" ? (
        <div className="flex justify-between border-t px-3 py-2 text-xs text-muted-foreground" onClick={onClose}>
          <span>{result?.truncated ? "More results exist; open the full page." : "Enter to open · Esc to close"}</span>
          <AppLink href={`/search?q=${encodeURIComponent(text.trim())}`} className="underline underline-offset-2">
            All results
          </AppLink>
        </div>
      ) : null}
    </div>
  );
}

/** Ctrl+K, Cmd+K and `/` open the palette from any view. */
export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      const chord = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k";
      const slash = event.key === "/" && !event.ctrlKey && !event.metaKey && !event.altKey && !typingInto(event.target);
      if (chord || slash) {
        event.preventDefault();
        onOpenChange(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onOpenChange]);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[720px]">
        <DialogTitle className="sr-only">Search</DialogTitle>
        <Palette onClose={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  );
}
