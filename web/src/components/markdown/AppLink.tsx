import { useRouter } from "@tanstack/react-router";
import { type ComponentProps, useEffect, useRef } from "react";
import { prefetchHref } from "@/api/prefetch";
import { internalClick } from "./nav";

/** A pointer that rests this long means the link is wanted; one passing over it costs nothing. */
const INTENT_MS = 120;

/** A link to a path inside the app, given as a string (`/items/tasks/CS-2#notes`). */
export function AppLink({ href, ...rest }: ComponentProps<"a"> & { href: string }) {
  const router = useRouter();
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const intend = () => {
    const client = router.options.context.queryClient;
    clearTimeout(timer.current);
    timer.current = setTimeout(() => prefetchHref(client, href), INTENT_MS);
  };
  const leave = () => clearTimeout(timer.current);

  return (
    <a
      href={href}
      onClick={internalClick((to) => router.history.push(to), href)}
      onMouseEnter={intend}
      onMouseLeave={leave}
      onFocus={intend}
      onBlur={leave}
      {...rest}
    />
  );
}
