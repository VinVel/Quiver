import { useState, useEffect } from "react";

export const useColorScheme = () => {
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    setIsDark(mediaQuery.matches);

    const listener = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mediaQuery.addEventListener("change", listener);

    return () => mediaQuery.removeEventListener("change", listener);
  }, []);

  return isDark ? "dark" : "light";
};
