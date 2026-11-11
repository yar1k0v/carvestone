import { type ReactNode, useState } from "react";
import { cn } from "@/lib/utils";

/** The head of a page: an optional mono line above the title, and what sits beside it. */
export function PageHeader({
  eyebrow,
  title,
  description,
  actions,
  className,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <header className={cn("mb-6 flex flex-wrap items-end gap-x-6 gap-y-2", className)}>
      <div className="min-w-0 flex-1">
        {eyebrow ? <p className="eyebrow mb-1">{eyebrow}</p> : null}
        <h1 className="text-2xl leading-8 font-[650]">{title}</h1>
        {description ? <p className="mt-2.5 max-w-2xl text-lg text-muted-foreground">{description}</p> : null}
      </div>
      {actions ? <div className="flex shrink-0 flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  );
}

/** One number with what it counts. */
export function Stat({ label, value, tone, hint }: { label: string; value: ReactNode; tone?: string; hint?: string }) {
  return (
    <div className="slab min-w-0 px-4 py-3" title={hint}>
      <p className="eyebrow truncate">{label}</p>
      <p className={cn("mt-0.5 text-2xl font-semibold tabular-nums tracking-tight", tone)}>{value}</p>
    </div>
  );
}

/** A long list that shows its first `limit` rows until asked for the rest. */
export function More<T>({
  rows,
  limit,
  label,
  className,
  children,
}: {
  rows: T[];
  limit: number;
  /** The closed button text, given the count of hidden rows; defaults to `Show all N`. */
  label?: (hidden: number) => string;
  /** Extra classes for the button; they win over the default text size and weight. */
  className?: string;
  children: (shown: T[]) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const shown = open ? rows : rows.slice(0, limit);
  return (
    <>
      {children(shown)}
      {rows.length > limit ? (
        <button
          type="button"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          className={cn("mt-1 rounded text-xs font-medium text-primary hover:underline", className)}
        >
          {open ? "Show fewer" : label === undefined ? `Show all ${rows.length}` : label(rows.length - limit)}
        </button>
      ) : null}
    </>
  );
}

/** Choose one of a few values; the chosen one is pressed. */
export function Chips<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string; count?: number }[];
  value: T;
  onChange: (next: T) => void;
}) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={option.value === value}
          onClick={() => onChange(option.value)}
          className="rounded-md border px-3 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
        >
          {option.label}
          {option.count === undefined ? null : <span className="ml-1.5 tabular-nums opacity-70">{option.count}</span>}
        </button>
      ))}
    </div>
  );
}
