import { Outlet, useLocation } from "@tanstack/react-router";
import { Menu, Monitor, Moon, Search, Sun, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { type EventsState, useEvents } from "@/api/events";
import { AppLink } from "@/components/markdown/AppLink";
import { PeekProvider } from "@/components/peek/PeekProvider";
import { CommandPalette } from "@/components/search/CommandPalette";
import { Button } from "@/components/ui/button";
import { Dialog, DialogTitle, SheetContent } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { Breadcrumbs } from "./Breadcrumbs";
import { SPACES, type Space, spaceOf, useBrand, useSpaceNav } from "./nav";
import { OnThisPage, OnThisPageFolded } from "./OnThisPage";
import { type Crumb, FrameContext, type FrameSetters, type OutlineEntry } from "./pageFrame";
import { Sidebar, SpaceLinks } from "./Sidebar";
import { type Theme, useTheme } from "./theme";
import { TopProgress } from "./TopProgress";

const WIDE_SCREEN = "(min-width: 1280px)";

/** True when the right column fits. A browser without `matchMedia`, like jsdom, counts as wide. */
function useWideScreen(): boolean {
  const [wide, setWide] = useState(() => typeof window.matchMedia !== "function" || window.matchMedia(WIDE_SCREEN).matches);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(WIDE_SCREEN);
    const change = () => setWide(query.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  }, []);
  return wide;
}

const THEME_ICON = { light: Sun, dark: Moon, system: Monitor } satisfies Record<Theme, typeof Sun>;

const STATUS_TEXT = { connecting: "connecting", online: "live", offline: "offline" } as const;

function Brand({ className }: { className?: string }) {
  const brand = useBrand();
  return (
    <AppLink href="/" className={cn("flex min-w-0 items-center gap-[9px] font-bold", className)}>
      <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true" className="shrink-0 text-primary">
        <path d="M2 3h14M2 9h9M2 15h14" stroke="currentColor" strokeWidth="2.4" />
      </svg>
      <span className="truncate">{brand}</span>
    </AppLink>
  );
}

/** The connection light and the theme button; they live beside Search because the boards have no other place for them. `large` is the 44px touch size of the phone bar. */
function Status({ events, large = false }: { events: EventsState; large?: boolean }) {
  const { theme, cycle } = useTheme();
  const ThemeIcon = THEME_ICON[theme];
  return (
    <>
      <span className="flex items-center gap-1.5 text-xs text-muted-foreground" role="status" aria-label={`Connection: ${STATUS_TEXT[events.status]}`}>
        <span
          aria-hidden
          className={cn(
            "size-2 rounded-full",
            events.status === "online" && "bg-emerald-500",
            events.status === "connecting" && "bg-amber-500",
            events.status === "offline" && "bg-destructive",
          )}
        />
        {STATUS_TEXT[events.status]}
      </span>
      {events.refreshError !== null &&
        (large ? (
          // Text here takes the width `Brand` needs on a phone.
          <span className="shrink-0 text-destructive" role="alert" title={events.refreshError} aria-label="index refresh failed">
            <TriangleAlert className="size-4" aria-hidden="true" />
          </span>
        ) : (
          <span className="min-w-0 max-w-40 truncate text-xs text-destructive" role="alert" title={events.refreshError}>
            index refresh failed
          </span>
        ))}
      <Button variant="ghost" size="icon" className={large ? "size-11" : "size-8"} onClick={cycle} aria-label={`Theme: ${theme}. Click to change.`} title={`Theme: ${theme}`}>
        <ThemeIcon />
      </Button>
    </>
  );
}

function TopBar({ space, events, onSearch }: { space: Space; events: EventsState; onSearch: () => void }) {
  return (
    <header className="hidden h-[52px] shrink-0 items-center gap-7 border-b bg-card px-6 md:flex">
      <Brand className="text-[15px]" />
      <nav aria-label="Spaces" className="flex gap-[22px] text-sm font-medium">
        {SPACES.map((entry) => (
          <AppLink
            key={entry.space}
            href={entry.href}
            aria-current={entry.space === space ? "page" : undefined}
            className={cn("transition-colors hover:text-primary", entry.space === space ? "font-semibold text-primary" : "text-muted-foreground")}
          >
            {entry.label}
          </AppLink>
        ))}
      </nav>
      <div className="ml-auto flex items-center gap-3">
        <button
          type="button"
          onClick={onSearch}
          className="flex items-center gap-2 rounded-[7px] border bg-background px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          <Search className="size-4" />
          Search
          <kbd aria-hidden className="ml-3 font-mono text-xs">
            Ctrl K
          </kbd>
        </button>
        <Status events={events} />
      </div>
    </header>
  );
}

function PhoneBar({ events, hasMenu, onMenu, onSearch }: { events: EventsState; hasMenu: boolean; onMenu: () => void; onSearch: () => void }) {
  return (
    <header className="flex shrink-0 items-center gap-2 border-b bg-card px-4 py-3.5 md:hidden">
      {hasMenu && (
        <Button variant="ghost" size="icon" className="size-11" aria-label="Open menu" onClick={onMenu}>
          <Menu />
        </Button>
      )}
      <Brand className="flex-1 text-[15px]" />
      <Status events={events} large />
      <Button variant="ghost" size="icon" className="size-11 text-muted-foreground" aria-label="Search" onClick={onSearch}>
        <Search className="size-5" />
      </Button>
    </header>
  );
}

function PhoneSpaces({ space }: { space: Space }) {
  return (
    <nav aria-label="Spaces" className="grid shrink-0 grid-cols-4 border-t bg-card text-[13px] font-medium md:hidden">
      {SPACES.map((entry) => (
        <AppLink
          key={entry.space}
          href={entry.href}
          aria-current={entry.space === space ? "page" : undefined}
          className={cn("pt-3.5 pb-4 text-center", entry.space === space ? "text-primary" : "text-muted-foreground")}
        >
          {entry.label}
        </AppLink>
      ))}
    </nav>
  );
}

/** The frame around every page: sidebar, breadcrumbs, centred content and "On this page". */
export function Shell() {
  const events = useEvents();
  const pathname = useLocation({ select: (location) => location.pathname });
  const hash = useLocation({ select: (location) => location.hash });
  const wideScreen = useWideScreen();
  const space = spaceOf(pathname);
  const entries = useSpaceNav(space, pathname);
  const [searching, setSearching] = useState(false);
  // Holds the address the drawer opened on, so moving to another page closes it.
  const [menuOn, setMenuOn] = useState<string | null>(null);
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  const [crumbs, setCrumbs] = useState<Crumb[] | null>(null);
  const [wide, setWide] = useState(false);
  const setters = useMemo<FrameSetters>(() => ({ outline: setOutline, crumbs: setCrumbs, wide: setWide }), []);
  const main = useRef<HTMLElement>(null);

  useEffect(() => {
    if (hash === "" && main.current !== null) main.current.scrollTop = 0;
  }, [pathname, hash]);

  const openSearch = () => setSearching(true);
  const hasOutline = outline.length > 0;

  return (
    <FrameContext.Provider value={setters}>
      <PeekProvider>
        <TopProgress />
        <div className="flex h-dvh flex-col">
          <TopBar space={space} events={events} onSearch={openSearch} />
          <div className="flex min-h-0 flex-1">
            <Sidebar entries={entries} />
            <div className="flex min-w-0 flex-1 flex-col">
              <PhoneBar events={events} hasMenu={entries.length > 0} onMenu={() => setMenuOn(pathname)} onSearch={openSearch} />
              <main ref={main} className="min-h-0 flex-1 overflow-y-auto">
                <div className={cn("mx-auto flex w-full justify-center gap-10 px-4 py-4 md:px-6 md:py-[22px]", !wide && "max-w-[1240px]")}>
                  <div className={cn("min-w-0 flex-1", !wide && hasOutline && "max-w-[860px]")}>
                    <Breadcrumbs override={crumbs} />
                    {hasOutline && !wideScreen ? <OnThisPageFolded entries={outline} /> : null}
                    <Outlet />
                  </div>
                  {hasOutline && wideScreen ? <OnThisPage entries={outline} /> : null}
                </div>
              </main>
            </div>
          </div>
          <PhoneSpaces space={space} />
        </div>
        {entries.length > 0 && (
          <Dialog open={menuOn === pathname} onOpenChange={(open) => setMenuOn(open ? pathname : null)}>
            <SheetContent>
              <DialogTitle className="sr-only">Menu</DialogTitle>
              <SpaceLinks entries={entries} />
            </SheetContent>
          </Dialog>
        )}
        <CommandPalette open={searching} onOpenChange={setSearching} />
      </PeekProvider>
    </FrameContext.Provider>
  );
}
