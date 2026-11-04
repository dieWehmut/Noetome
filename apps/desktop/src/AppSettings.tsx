import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon";
import type { Page, SettingsSection, Theme } from "./appTypes";

type CloseBehavior = "hide_to_tray" | "exit";
type SettingsPhase = "loading" | "ready" | "saving" | "saved" | "error";

function readBehavior(value: unknown): CloseBehavior {
  if (value === "hide_to_tray" || value === "exit") return value;
  throw new Error("DESKTOP_SETTINGS_INVALID_RESPONSE：没有收到可用的窗口设置。请重新读取设置。");
}

function settingsErrorText(error: unknown) {
  if (!isTauri()) return "DESKTOP_SETTINGS_UNAVAILABLE：当前预览不能读取或保存窗口设置。请在已安装的 Noetome 中设置。";
  const detail = typeof error === "string" ? error : error instanceof Error ? error.message : "本地组件没有返回可用结果。";
  return detail.startsWith("DESKTOP_") ? detail : `DESKTOP_SETTINGS_ERROR：${detail}`;
}

function WindowBehaviorSettings() {
  const [savedBehavior, setSavedBehavior] = useState<CloseBehavior | null>(null);
  const [draftBehavior, setDraftBehavior] = useState<CloseBehavior | "">("");
  const [phase, setPhase] = useState<SettingsPhase>("loading");
  const [error, setError] = useState("");
  const requestSequence = useRef(0);
  const labels = { hide_to_tray: "隐藏到托盘", exit: "直接退出" };
  const busy = phase === "loading" || phase === "saving";
  const changed = savedBehavior !== null && draftBehavior !== savedBehavior;

  async function loadSettings() {
    const request = ++requestSequence.current;
    setPhase("loading");
    setError("");
    setSavedBehavior(null);
    setDraftBehavior("");
    try {
      const behavior = readBehavior(await invoke<unknown>("get_close_behavior"));
      if (request !== requestSequence.current) return;
      setSavedBehavior(behavior);
      setDraftBehavior(behavior);
      setPhase("ready");
    } catch (cause) {
      if (request !== requestSequence.current) return;
      setError(settingsErrorText(cause));
      setPhase("error");
    }
  }

  useEffect(() => {
    void loadSettings();
    return () => { requestSequence.current += 1; };
  }, []);

  async function saveSettings() {
    if (busy || !changed || draftBehavior === "") return;
    const request = ++requestSequence.current;
    setPhase("saving");
    setError("");
    try {
      const behavior = readBehavior(await invoke<unknown>("set_close_behavior", { behavior: draftBehavior }));
      if (request !== requestSequence.current) return;
      setSavedBehavior(behavior);
      setDraftBehavior(behavior);
      setPhase("saved");
    } catch (cause) {
      if (request !== requestSequence.current) return;
      setError(settingsErrorText(cause));
      setPhase("error");
    }
  }

  const status = phase === "loading" ? "正在读取窗口设置…"
    : phase === "saving" ? "正在保存设置…"
    : savedBehavior === null ? "尚未读取窗口设置。"
    : changed ? `修改尚未保存。当前设置：${labels[savedBehavior]}。`
    : phase === "saved" ? `已保存：${labels[savedBehavior]}。`
    : `当前设置：${labels[savedBehavior]}。`;

  return (
    <section className="window-settings settings-section" aria-labelledby="window-settings-title" aria-busy={busy}>
      <h2 id="window-settings-title" tabIndex={-1}>窗口行为</h2>
      <p className="settings-description" id="window-behavior-description">选择关闭窗口后的操作。</p>
      <div className="settings-row">
        <div className="settings-row-copy"><label htmlFor="close-behavior">关闭窗口时</label><p>最小化后仍显示在任务栏；托盘菜单“退出”始终结束程序。</p></div>
        <div className="settings-controls">
          <select id="close-behavior" value={draftBehavior} disabled={busy || savedBehavior === null} aria-describedby="window-behavior-description window-settings-status" onChange={(event) => {
            setDraftBehavior(readBehavior(event.target.value));
            setError("");
            setPhase("ready");
          }}>
            <option value="" disabled>尚未读取</option>
            <option value="hide_to_tray">隐藏到托盘（默认）</option>
            <option value="exit">直接退出</option>
          </select>
        </div>
      </div>
      <div className="settings-save-row">
        <p className="settings-status" id="window-settings-status" role="status" aria-live="polite">{status}</p>
        {savedBehavior === null
          ? <button className="button button-secondary" disabled={busy} onClick={() => void loadSettings()}>{busy ? "正在读取…" : "重新读取"}</button>
          : <button className="button button-secondary" disabled={busy || !changed} onClick={() => void saveSettings()}>{phase === "saving" ? "正在保存…" : "保存设置"}</button>}
      </div>
      {error && <p className="error-message" role="alert">{error}</p>}
    </section>
  );
}

const sections: { id: SettingsSection; label: string; icon: "sun" | "window" | "info" }[] = [
  { id: "appearance", label: "外观", icon: "sun" },
  { id: "window", label: "窗口行为", icon: "window" },
  { id: "about", label: "关于 Noetome", icon: "info" },
];

interface AppSettingsProps {
  section: SettingsSection;
  theme: Theme;
  themeSaved: boolean;
  version?: string;
  onThemeChange: (theme: Theme) => void;
  onNavigate: (page: Page) => void;
}

export default function AppSettings({ section, theme, themeSaved, version, onThemeChange, onNavigate }: AppSettingsProps) {
  return (
    <div className="settings-page">
      <div className="settings-page-heading"><AppIcon name="settings" /><h1>设置</h1></div>
      <div className="settings-layout">
        <nav className="settings-navigation" aria-label="设置分类">
          {sections.map((item) => <button key={item.id} className={`settings-nav-item ${section === item.id ? "is-active" : ""}`} aria-current={section === item.id ? "page" : undefined} onClick={() => onNavigate(item.id)}><AppIcon name={item.icon} /><span>{item.label}</span></button>)}
        </nav>
        <div className="settings-content">
          {section === "appearance" && <section className="settings-section" aria-labelledby="appearance-settings-title">
            <h2 id="appearance-settings-title" tabIndex={-1}>外观</h2>
            <p className="settings-description">让工作台符合你的阅读习惯。</p>
            <div className="settings-row">
              <div className="settings-row-copy"><label htmlFor="appearance-theme">主题</label><p>也可以点击左下角的太阳或月亮切换。</p></div>
              <div className="settings-controls"><select id="appearance-theme" value={theme} onChange={(event) => { const value = event.target.value; if (value === "light" || value === "dark") onThemeChange(value); }}><option value="light">亮色</option><option value="dark">暗色</option></select></div>
            </div>
            <p className="settings-status" role="status" aria-live="polite">{themeSaved ? "主题已保存，下次打开时继续使用。" : `当前主题：${theme === "light" ? "亮色" : "暗色"}。`}</p>
          </section>}
          {section === "window" && <WindowBehaviorSettings />}
          {section === "about" && <section className="settings-section settings-about" aria-labelledby="about-settings-title">
            <h2 id="about-settings-title" tabIndex={-1}>关于 Noetome</h2>
            <p className="settings-description">个人学科学习知识编译器</p>
            <div className="settings-about-brand"><img src="/app-icon.png" alt="Noetome 应用图标" /><div><h3>Noetome</h3><p>从电子资料到可追溯的 Obsidian 知识库。</p></div></div>
            <div className="settings-row"><div className="settings-row-copy"><h3>应用版本</h3><p>{version ?? "尚未读取版本"}</p></div><button className="button button-secondary" onClick={() => onNavigate("runtime")}>查看运行信息</button></div>
            <div className="settings-about-note"><p>界面、字体与图标随安装包提供。窗口设置由本地组件管理，主题保存在本机。</p><p>当前桌面提供帮助、版本与连接检查。项目和资料操作暂通过 CLI 使用。</p></div>
          </section>}
        </div>
      </div>
    </div>
  );
}
