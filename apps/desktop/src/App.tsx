import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import AppMenuBar from "./AppMenuBar";
import type { AppMenu } from "./AppMenuBar";
import AppSidebar from "./AppSidebar";
import AppSettings from "./AppSettings";
import Icon from "./AppIcon";
import useThemePreference from "./useThemePreference";
import type { Page } from "./appTypes";

type Phase = "idle" | "running" | "success" | "error";
type PreviewOption = "help" | "version";

interface RuntimeInfo {
  app_version: string;
  cli_version: string;
  target: string;
  capabilities: string[];
  cli_help: string;
}

interface Preview {
  phase: Phase;
  option?: PreviewOption;
  output: string;
  error: string;
}

const pages: { id: Page; label: string; icon: "home" | "pulse" | "info" | "settings" }[] = [
  { id: "workspace", label: "工作台", icon: "home" },
  { id: "runtime", label: "运行检查", icon: "pulse" },
  { id: "appearance", label: "设置", icon: "settings" },
  { id: "about", label: "关于", icon: "info" },
];

function errorText(error: unknown) {
  if (!isTauri()) return "DESKTOP_IPC_ERROR：当前预览没有桌面运行环境。请在已安装的 Noetome 中运行检查。";
  const detail = typeof error === "string" ? error : error instanceof Error ? error.message : "本地组件没有返回可用结果。";
  return `DESKTOP_IPC_ERROR：${detail} 请重新打开 Noetome 后重试。`;
}

function StateBadge({ phase }: { phase: Phase }) {
  const labels = { idle: "未运行", running: "检查中", success: "连接正常", error: "检查失败" };
  return <span className={`state-badge state-${phase}`}><span className="state-dot" />{labels[phase]}</span>;
}

export default function App() {
  const [history, setHistory] = useState<{ entries: Page[]; index: number }>({ entries: ["workspace"], index: 0 });
  const page = history.entries[history.index];
  const theme = useThemePreference();
  const recentPages = [...new Set(history.entries.slice(0, history.index + 1).reverse())].slice(0, 5);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [focusTarget, setFocusTarget] = useState<{ id: string; request: number } | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  const [hasSelection, setHasSelection] = useState(false);
  const [editFeedback, setEditFeedback] = useState<{ message: string; error: boolean } | null>(null);
  const [runtimePhase, setRuntimePhase] = useState<Phase>("idle");
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfo | null>(null);
  const [runtimeError, setRuntimeError] = useState("");
  const [preview, setPreview] = useState<Preview>({ phase: "idle", output: "", error: "" });

  function requestFocus(id = "main-content") {
    setFocusTarget((previous) => ({ id, request: (previous?.request ?? 0) + 1 }));
  }

  function navigate(target: Page, focusId = "main-content") {
    setHistory((previous) => {
      if (previous.entries[previous.index] === target) return previous;
      const entries = [...previous.entries.slice(0, previous.index + 1), target].slice(-64);
      return { entries, index: entries.length - 1 };
    });
    setEditFeedback(null);
    requestFocus(focusId);
  }

  function moveHistory(direction: -1 | 1) {
    const next = history.index + direction;
    if (next < 0 || next >= history.entries.length) return;
    setHistory((previous) => ({ ...previous, index: previous.index + direction }));
    setEditFeedback(null);
    requestFocus();
  }

  useEffect(() => {
    if (!focusTarget) return;
    const target = document.getElementById(focusTarget.id) ?? mainRef.current;
    target?.focus({ preventScroll: true });
    if (target === mainRef.current) mainRef.current?.scrollTo({ top: 0 });
    else target?.scrollIntoView({ block: "start" });
  }, [page, focusTarget]);

  function toggleSidebar() {
    if (sidebarVisible && document.getElementById("app-sidebar")?.contains(document.activeElement)) {
      mainRef.current?.focus({ preventScroll: true });
    }
    setSidebarVisible((visible) => !visible);
  }

  function selectedMainText() {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !mainRef.current || !selection.rangeCount) return "";
    const range = selection.getRangeAt(0);
    if (!mainRef.current.contains(range.startContainer) || !mainRef.current.contains(range.endContainer)) return "";
    return selection.toString();
  }

  useEffect(() => {
    function updateSelection() { setHasSelection(selectedMainText().length > 0); }
    document.addEventListener("selectionchange", updateSelection);
    return () => document.removeEventListener("selectionchange", updateSelection);
  }, []);

  function selectAllContent() {
    const main = mainRef.current;
    const selection = window.getSelection();
    if (!main || !selection) return;
    main.focus({ preventScroll: true });
    const range = document.createRange();
    range.selectNodeContents(main);
    selection.removeAllRanges();
    selection.addRange(range);
    setHasSelection(selectedMainText().length > 0);
  }

  async function copySelectedContent() {
    const text = selectedMainText();
    if (!text) {
      setEditFeedback({ message: "请先选中当前内容中需要复制的文字。", error: true });
      return;
    }
    try {
      if (!navigator.clipboard?.writeText) throw new Error("当前环境不支持剪贴板写入。请使用系统复制快捷键。");
      await navigator.clipboard.writeText(text);
      setEditFeedback({ message: "已复制选中的文字。", error: false });
    } catch (cause) {
      const detail = cause instanceof Error ? cause.message : String(cause);
      setEditFeedback({ message: `复制失败：${detail}`, error: true });
    }
  }

  async function checkRuntime() {
    setRuntimePhase("running");
    setRuntimeError("");
    setRuntimeInfo(null);
    try {
      const result = await invoke<RuntimeInfo>("runtime_info");
      setRuntimeInfo(result);
      setRuntimePhase("success");
    } catch (error) {
      setRuntimeError(errorText(error));
      setRuntimePhase("error");
    }
  }

  async function runPreview(option: PreviewOption) {
    if (preview.phase === "running") return;
    setPreview({ phase: "running", option, output: "", error: "" });
    try {
      const output = await invoke<string>("cli_preview", { option });
      setPreview({ phase: "success", option, output, error: "" });
    } catch (error) {
      setPreview({ phase: "error", option, output: "", error: errorText(error) });
    }
  }

  const menus: AppMenu[] = [
    { label: "File", actions: [
      { id: "workspace", label: "工作台", onSelect: () => navigate("workspace") },
      { id: "runtime", label: "运行检查", onSelect: () => navigate("runtime") },
      { id: "appearance", label: "设置", onSelect: () => navigate("appearance") },
      { id: "settings", label: "窗口设置", onSelect: () => navigate("window", "window-settings-title") },
    ] },
    { label: "Edit", actions: [
      { id: "select-all", label: "全选", onSelect: selectAllContent },
      { id: "copy", label: "复制", disabled: !hasSelection, onSelect: () => void copySelectedContent() },
    ] },
    { label: "View", actions: [
      { id: "sidebar", label: "显示侧栏", checked: sidebarVisible, onSelect: toggleSidebar },
      ...pages.map((item) => ({ id: item.id, label: item.label, onSelect: () => navigate(item.id) })),
    ] },
    { label: "Help", actions: [
      { id: "help", label: "查看帮助", disabled: preview.phase === "running", onSelect: () => { navigate("runtime", "command-title"); void runPreview("help"); } },
      { id: "version", label: "查看版本", disabled: preview.phase === "running", onSelect: () => { navigate("runtime", "command-title"); void runPreview("version"); } },
      { id: "about", label: "关于 Noetome", onSelect: () => navigate("about") },
    ] },
  ];

  return (
    <div className="app-layout">
      <AppMenuBar canGoBack={history.index > 0} canGoForward={history.index < history.entries.length - 1} sidebarVisible={sidebarVisible} onBack={() => moveHistory(-1)} onForward={() => moveHistory(1)} onToggleSidebar={toggleSidebar} menus={menus} />
      <div className="app-body">
      <AppSidebar page={page} visible={sidebarVisible} recentPages={recentPages} theme={theme.theme} onNavigate={navigate} onToggleTheme={() => theme.setTheme(theme.theme === "light" ? "dark" : "light")} />

      <div className="main-layout">
        <main id="main-content" className="main-content" tabIndex={-1} ref={mainRef}>
          {theme.error && <p className="error-message theme-feedback" role="alert">{theme.error}</p>}
          {editFeedback && <p className={editFeedback.error ? "error-message edit-feedback" : "edit-feedback"} role={editFeedback.error ? "alert" : "status"}>{editFeedback.message}</p>}
          {page === "workspace" && (
            <>
              <div className="page-heading"><div><p className="eyebrow">YOUR KNOWLEDGE SPACE</p><h1>为理解，留一个空间。</h1><p className="page-description">从资料出发，让知识逐渐成为你的体系。</p></div></div>

              <section className="welcome-card" aria-labelledby="welcome-title">
                <div className="welcome-copy"><span className="section-kicker">认识你的工作台</span><h2 id="welcome-title">把阅读的线索，<br />连接成清晰的知识。</h2><p>Noetome 面向个人学科学习，将资料整理为<br className="desktop-break" />可追溯、可维护的 Obsidian 知识库。</p><button className="button button-primary" onClick={() => navigate("runtime")}>检查本地运行环境<Icon name="arrow" /></button></div>
                <div className="welcome-art" aria-hidden="true"><img src="/app-icon.png" alt="" /><span className="art-label">让理解慢慢生长</span></div>
              </section>

              <div className="section-title-row"><h2>当前可以做什么</h2><span>从小而可靠的能力开始</span></div>
              <div className="capability-grid">
                <section className="capability-card"><div className="card-icon"><Icon name="terminal" /></div><span className="small-badge available">已接入</span><h3>查看帮助与版本</h3><p>通过本地组件查看 CLI 用法与版本，确认桌面应用连接状态。</p><button className="text-button" onClick={() => navigate("runtime")}>打开运行检查<Icon name="arrow" /></button></section>
                <section className="capability-card"><div className="card-icon muted"><Icon name="book" /></div><span className="small-badge pending">尚未接入</span><h3>建立学习项目</h3><p>项目与资料操作暂通过 CLI 使用，桌面入口尚未接入。当前工作台尚未载入项目。</p><span className="quiet-note">完成后将在这里开始</span></section>
              </div>

              <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>桌面项目、资料与笔记生成入口仍在开发中，现有核心能力可通过 CLI 使用。</p></section>
            </>
          )}

          {page === "runtime" && (
            <>
              <div className="page-heading"><div><p className="eyebrow">LOCAL RUNTIME</p><h1>检查本地连接。</h1><p className="page-description">查看应用与 Rust 组件的实际状态，试运行已有命令。</p></div></div>

              <section className="runtime-card" aria-labelledby="runtime-title">
                <div className="runtime-card-heading"><div className="runtime-title"><div className="card-icon"><Icon name="pulse" /></div><div><h2 id="runtime-title">本地运行环境</h2><p>点击检查后读取实际版本和当前能力。</p></div></div><StateBadge phase={runtimePhase} /></div>
                <dl className="runtime-details">
                  <div><dt>桌面应用版本</dt><dd>{runtimeInfo?.app_version ?? "尚未读取"}</dd></div>
                  <div><dt>CLI 版本</dt><dd>{runtimeInfo?.cli_version ?? "尚未读取"}</dd></div>
                  <div><dt>构建目标</dt><dd>{runtimeInfo?.target ?? "尚未读取"}</dd></div>
                  <div><dt>已接入能力</dt><dd>{runtimeInfo ? runtimeInfo.capabilities.join(" · ") : "尚未读取"}</dd></div>
                </dl>
                <div className="runtime-actions"><button className="button button-primary" disabled={runtimePhase === "running"} onClick={checkRuntime}>{runtimePhase === "running" ? "正在检查…" : runtimePhase === "idle" ? "检查连接" : "重新检查"}<Icon name="arrow" /></button><span role="status" aria-live="polite">{runtimePhase === "success" ? "已收到本地 Rust 组件的实际返回结果。" : runtimePhase === "running" ? "正在读取本地运行信息。" : "检查只读取状态，不创建项目或资料。"}</span></div>
                {runtimeError && <p className="error-message" role="alert">{runtimeError}</p>}
              </section>

              <section className="command-card" aria-labelledby="command-title">
                <div className="command-heading"><div><h2 id="command-title" tabIndex={-1}>试运行 CLI</h2><p>显示帮助或版本，结果直接来自本地组件。</p></div><div className="command-actions"><button className="button button-secondary" disabled={preview.phase === "running"} onClick={() => runPreview("help")}>查看帮助</button><button className="button button-secondary" disabled={preview.phase === "running"} onClick={() => runPreview("version")}>查看版本</button></div></div>
                <div className="console-panel"><div className="console-heading"><span><Icon name="terminal" />{preview.option ? `noetome --${preview.option}` : "命令输出"}</span><span className={`console-status ${preview.phase}`} role="status" aria-live="polite">{{ idle: "未运行", running: "运行中", success: "执行成功", error: "执行失败" }[preview.phase]}</span></div><pre aria-label="CLI 运行结果" tabIndex={0}>{preview.phase === "idle" ? "选择上方命令，查看实际输出。" : preview.phase === "running" ? "等待本地组件返回…" : preview.phase === "error" ? preview.error : preview.output}</pre></div>
              </section>
              <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>帮助和版本检查不执行资料解析、模型调用或 Vault 写入。</p></section>
            </>
          )}

          {(page === "appearance" || page === "window" || page === "about") && <AppSettings section={page} theme={theme.theme} themeSaved={theme.saved} version={runtimeInfo?.app_version} onThemeChange={theme.setTheme} onNavigate={navigate} />}

        </main>

        <footer className="app-footer"><span>Noetome · 为个人学习而构建</span><span>从清晰的来源，走向持续的理解。</span></footer>
      </div>
      </div>
    </div>
  );
}
