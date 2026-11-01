import { useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

type Page = "workspace" | "runtime" | "about";
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

const pages: { id: Page; label: string; icon: "home" | "pulse" | "info" }[] = [
  { id: "workspace", label: "工作台", icon: "home" },
  { id: "runtime", label: "运行检查", icon: "pulse" },
  { id: "about", label: "关于", icon: "info" },
];

function Icon({ name }: { name: "home" | "pulse" | "info" | "arrow" | "book" | "terminal" }) {
  const paths = {
    home: <><path d="m3 10 9-7 9 7" /><path d="M5 9v11h14V9M9 20v-7h6v7" /></>,
    pulse: <><path d="M3 12h4l3-7 4 14 3-7h4" /></>,
    info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7v.1" /></>,
    arrow: <><path d="M5 12h14m-6-6 6 6-6 6" /></>,
    book: <><path d="M4 4h6a4 4 0 0 1 4 4v12a4 4 0 0 0-4-3H4z" /><path d="M14 8a4 4 0 0 1 4-4h2v13h-2a4 4 0 0 0-4 3" /></>,
    terminal: <><rect x="3" y="4" width="18" height="16" rx="3" /><path d="m7 9 3 3-3 3m6 0h4" /></>,
  };

  return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>;
}

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
  const [page, setPage] = useState<Page>("workspace");
  const [runtimePhase, setRuntimePhase] = useState<Phase>("idle");
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfo | null>(null);
  const [runtimeError, setRuntimeError] = useState("");
  const [preview, setPreview] = useState<Preview>({ phase: "idle", output: "", error: "" });

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
    setPreview({ phase: "running", option, output: "", error: "" });
    try {
      const output = await invoke<string>("cli_preview", { option });
      setPreview({ phase: "success", option, output, error: "" });
    } catch (error) {
      setPreview({ phase: "error", option, output: "", error: errorText(error) });
    }
  }

  const title = { workspace: "工作台", runtime: "运行检查", about: "关于 Noetome" }[page];

  return (
    <div className="app-layout">
      <aside className="sidebar" aria-label="应用导航">
        <a className="brand" href="#workspace" onClick={(event) => { event.preventDefault(); setPage("workspace"); }} aria-label="Noetome 工作台">
          <img src="/app-icon.png" alt="" className="brand-icon" />
          <span>Noetome<small>个人知识工作台</small></span>
        </a>

        <div className="nav-section-label">你的空间</div>
        <nav>
          {pages.map((item) => (
            <button key={item.id} className={`nav-item ${page === item.id ? "is-active" : ""}`} aria-current={page === item.id ? "page" : undefined} onClick={() => setPage(item.id)}>
              <Icon name={item.icon} /><span>{item.label}</span>{page === item.id && <span className="nav-active-dot" />}
            </button>
          ))}
        </nav>

        <div className="sidebar-note">
          <span className="sidebar-note-label">一步一步，积累理解</span>
          <p>让资料成为相互连接、<br />可以追溯的知识。</p>
          <div className="local-label"><span />本地桌面应用</div>
        </div>
      </aside>

      <div className="main-layout">
        <header className="topbar">
          <div className="breadcrumb">Noetome<span>/</span><strong>{title}</strong></div>
          <span className="build-tag">早期桌面版</span>
        </header>

        <main id="main-content" className="main-content">
          {page === "workspace" && (
            <>
              <div className="page-heading"><div><p className="eyebrow">YOUR KNOWLEDGE SPACE</p><h1>为理解，留一个空间。</h1><p className="page-description">从资料出发，让知识逐渐成为你的体系。</p></div><span className="heading-decoration" aria-hidden="true">✳</span></div>

              <section className="welcome-card" aria-labelledby="welcome-title">
                <div className="welcome-copy"><span className="section-kicker">认识你的工作台</span><h2 id="welcome-title">把阅读的线索，<br />连接成清晰的知识。</h2><p>Noetome 面向个人学科学习，将资料整理为<br className="desktop-break" />可追溯、可维护的 Obsidian 知识库。</p><button className="button button-primary" onClick={() => setPage("runtime")}>检查本地运行环境<Icon name="arrow" /></button></div>
                <div className="welcome-art" aria-hidden="true"><div className="art-ring art-ring-outer" /><div className="art-ring art-ring-inner" /><img src="/app-icon.png" alt="" /><span className="art-label">让理解慢慢生长</span></div>
              </section>

              <div className="section-title-row"><h2>当前可以做什么</h2><span>从小而可靠的能力开始</span></div>
              <div className="capability-grid">
                <section className="capability-card"><div className="card-icon"><Icon name="terminal" /></div><span className="small-badge available">已接入</span><h3>查看帮助与版本</h3><p>通过本地组件查看 CLI 用法与版本，确认桌面应用连接状态。</p><button className="text-button" onClick={() => setPage("runtime")}>打开运行检查<Icon name="arrow" /></button></section>
                <section className="capability-card"><div className="card-icon muted"><Icon name="book" /></div><span className="small-badge pending">尚未实现</span><h3>建立学习项目</h3><p>项目初始化和资料导入仍在开发中。当前没有已创建的项目。</p><span className="quiet-note">完成后将在这里开始</span></section>
              </div>

              <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>这是一份桌面应用骨架。资料解析、知识抽取与笔记生成尚未就绪。</p></section>
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
                <div className="command-heading"><div><h2 id="command-title">试运行 CLI</h2><p>显示帮助或版本，结果直接来自本地组件。</p></div><div className="command-actions"><button className="button button-secondary" disabled={preview.phase === "running"} onClick={() => runPreview("help")}>查看帮助</button><button className="button button-secondary" disabled={preview.phase === "running"} onClick={() => runPreview("version")}>查看版本</button></div></div>
                <div className="console-panel"><div className="console-heading"><span><Icon name="terminal" />{preview.option ? `noetome --${preview.option}` : "命令输出"}</span><span className={`console-status ${preview.phase}`} role="status" aria-live="polite">{{ idle: "未运行", running: "运行中", success: "执行成功", error: "执行失败" }[preview.phase]}</span></div><pre aria-label="CLI 运行结果" tabIndex={0}>{preview.phase === "idle" ? "选择上方命令，查看实际输出。" : preview.phase === "running" ? "等待本地组件返回…" : preview.phase === "error" ? preview.error : preview.output}</pre></div>
              </section>
              <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>帮助和版本检查不执行资料解析、模型调用或 Vault 写入。</p></section>
            </>
          )}

          {page === "about" && (
            <>
              <div className="page-heading"><div><p className="eyebrow">ABOUT NOETOME</p><h1>让知识，有迹可循。</h1><p className="page-description">一款为个人学科学习而构建的知识编译器。</p></div></div>
              <section className="about-card"><img src="/app-icon.png" alt="Noetome 应用图标" className="about-icon" /><div><span className="section-kicker">PERSONAL KNOWLEDGE COMPILER</span><h2>Noetome</h2><p>从电子资料到 Obsidian 知识库，<br />让整理有依据，让理解能延续。</p><span className="about-version">{runtimeInfo ? `桌面应用 ${runtimeInfo.app_version}` : "早期桌面版 · 版本尚未读取"}</span></div></section>
              <div className="about-details"><section><h2>现在的 Noetome</h2><p>当前提供本地桌面界面，以及帮助、版本和运行连接检查。项目初始化、资料导入和知识生成尚未实现。</p><button className="text-button" onClick={() => setPage("runtime")}>查看实际运行信息<Icon name="arrow" /></button></section><section><h2>在你的电脑上</h2><p>应用界面和图标随安装包提供，不需要外部字体或网页服务。当前操作读取本地运行信息，不发起模型请求。</p><span className="local-label"><span />本地运行</span></section></div>
            </>
          )}
        </main>

        <footer className="app-footer"><span>Noetome · 为个人学习而构建</span><span>从清晰的来源，走向持续的理解。</span></footer>
      </div>
    </div>
  );
}
