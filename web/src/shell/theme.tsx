import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from "react";

export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "stone-theme";
const ORDER: Theme[] = ["light", "dark", "system"];

function read(): Theme {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark" || saved === "system") return saved;
  } catch {
    // Storage can be blocked; fall through to the default.
  }
  return "system";
}

function prefersDark(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
}

function apply(theme: Theme): void {
  const dark = theme === "dark" || (theme === "system" && prefersDark());
  document.documentElement.classList.toggle("dark", dark);
}

interface ThemeValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  /** Light, then dark, then system, then light again. */
  cycle: () => void;
}

const ThemeContext = createContext<ThemeValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(read);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // The choice still applies for this session.
    }
  }, []);

  const cycle = useCallback(() => {
    setTheme(ORDER[(ORDER.indexOf(theme) + 1) % ORDER.length] ?? "system");
  }, [theme, setTheme]);

  useEffect(() => {
    apply(theme);
    if (theme !== "system" || typeof matchMedia !== "function") return;
    const query = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => apply("system");
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [theme]);

  const value = useMemo(() => ({ theme, setTheme, cycle }), [theme, setTheme, cycle]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeValue {
  const value = useContext(ThemeContext);
  if (value === null) throw new Error("useTheme needs a ThemeProvider above it");
  return value;
}
