import { useEffect, useState } from "react";
import type { Theme } from "./appTypes";

const storageKey = "noetome.appearance.theme";

function readTheme(): { theme: Theme; error: string } {
  try {
    const value = window.localStorage.getItem(storageKey);
    if (value === null || value === "light" || value === "dark") {
      return { theme: value ?? "light", error: "" };
    }
    return { theme: "light", error: "无法识别已保存的主题。请选择亮色或暗色重新保存。" };
  } catch {
    return { theme: "light", error: "无法读取外观设置。请检查应用的本地存储后重试。" };
  }
}

export default function useThemePreference() {
  const [preference, setPreference] = useState(readTheme);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    document.documentElement.dataset.theme = preference.theme;
  }, [preference.theme]);

  useEffect(() => {
    function syncTheme(event: StorageEvent) {
      if (event.storageArea === window.localStorage && (event.key === storageKey || event.key === null)) {
        setPreference(readTheme());
        setSaved(false);
      }
    }
    window.addEventListener("storage", syncTheme);
    return () => window.removeEventListener("storage", syncTheme);
  }, []);

  function setTheme(theme: Theme) {
    try {
      window.localStorage.setItem(storageKey, theme);
      setPreference({ theme, error: "" });
      setSaved(true);
    } catch {
      setPreference((current) => ({ ...current, error: "主题未保存。请检查应用的本地存储后重试。" }));
      setSaved(false);
    }
  }

  return { theme: preference.theme, error: preference.error, saved, setTheme };
}
