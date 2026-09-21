import { useEffect, type ReactNode } from "react";
import { useThemeStore } from "./theme-store";

function resolveDark(theme: string): boolean {
  if (theme === "dark") return true;
  if (theme === "light") return false;
  return typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const theme = useThemeStore((state) => state.theme);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolveDark(theme));
  }, [theme]);

  return <>{children}</>;
}
