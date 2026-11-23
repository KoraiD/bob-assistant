import { useCallback, useEffect, useState } from "react";

/** UI-2: dark by default, optional "daylight", or follow macOS. Stored locally in the web view. */
export type Theme = "dark" | "daylight" | "system";
export const THEME_KEY = "bob.theme";

export const THEMES: { id: Theme; label: string }[] = [
  { id: "dark", label: "Midnight (default)" },
  { id: "daylight", label: "Daylight" },
  { id: "system", label: "Match macOS" },
];

/** The saved theme. `?theme=` in the URL wins (used by scripts/screenshot.sh). */
export function loadTheme(storage: Pick<Storage, "getItem"> = localStorage, search = window.location.search): Theme {
  const v = new URLSearchParams(search).get("theme") ?? storage.getItem(THEME_KEY);
  return v === "daylight" || v === "system" ? v : "dark";
}

/** Whether the daylight palette applies for `theme` given the OS preference. */
export function isDaylight(theme: Theme, osPrefersLight: boolean): boolean {
  return theme === "daylight" || (theme === "system" && osPrefersLight);
}

function osPrefersLight(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: light)").matches;
}

export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  root.classList.toggle("daylight", isDaylight(theme, osPrefersLight()));
}

/** Current theme, a setter that persists and applies it, and live tracking of the OS setting. */
export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setThemeState] = useState<Theme>(() => loadTheme());
  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => applyTheme(theme);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme]);
  const setTheme = useCallback((t: Theme) => {
    localStorage.setItem(THEME_KEY, t);
    setThemeState(t);
  }, []);
  return [theme, setTheme];
}
