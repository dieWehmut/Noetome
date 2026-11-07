import { useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import Icon from "./AppIcon";

type Phase = "idle" | "running" | "success" | "error";

/** Fields exactly as the Rust command returns them; nothing is recomputed here. */
interface ProjectSummary {
  path: string;
  project_id: string;
  schema_version: number;
  created_at: string;
  state: string;
  database: string;
}

const knownCodes = [
  "PROJECT_PATH_EXISTS",
  "PROJECT_INCOMPLETE",
  "PROJECT_INVALID_CONFIG",
  "PROJECT_INVALID_DATABASE",
  "PROJECT_UNSAFE_PATH",
  "PROJECT_DATABASE_BUSY",
  "PROJECT_IO_ERROR",
  "DESKTOP_PROJECT_DIALOG_UNAVAILABLE",
];

function projectErrorText(error: unknown) {
  if (!isTauri()) return "DESKTOP_IPC_ERROR：当前预览没有桌面运行环境。请在已安装的 Noetome 中打开项目。";
  const detail = typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : "本地组件没有返回可用结果。";
  // The Rust side already answers with `CODE：说明`. Keep that text verbatim so
  // the code stays greppable, and only wrap messages that carry no code.
  if (knownCodes.some((code) => detail.startsWith(`${code}：`))) return detail;
  return `DESKTOP_PROJECT_ERROR：${detail}`;
}

export default function AppProject() {
  const [path, setPath] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [summary, setSummary] = useState<ProjectSummary | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const busy = phase === "running";
  const trimmed = path.trim();

  async function pickDirectory() {
    if (busy) return;
    setNotice("");
    try {
      const picked = await invoke<string | null>("pick_directory");
      if (picked) setPath(picked);
      else setNotice("没有选择目录，项目路径保持不变。");
    } catch (cause) {
      setError(projectErrorText(cause));
      setPhase("error");
    }
  }

  async function run(command: "project_create" | "project_open") {
    if (busy) return;
    if (!trimmed) {
      setPhase("error");
      setSummary(null);
      setError("请先选择或输入项目目录。新建项目需要尚不存在的新目录，打开项目需要已存在的项目目录。");
      return;
    }
    setPhase("running");
    setError("");
    setNotice("");
    setSummary(null);
    try {
      const result = await invoke<ProjectSummary>(command, { path: trimmed });
      setSummary(result);
      setPhase("success");
    } catch (cause) {
      setError(projectErrorText(cause));
      setPhase("error");
    }
  }

  const labels = { idle: "未操作", running: "处理中", success: "已读取", error: "已拒绝" };

  return (
    <>
      <div className="page-heading"><div><p className="eyebrow">PROJECT</p><h1>建立或打开一个项目。</h1><p className="page-description">项目根由本地 Rust 组件创建与检查；桌面只显示它真实返回的字段。</p></div></div>

      <section className="runtime-card" aria-labelledby="project-title">
        <div className="runtime-card-heading">
          <div className="runtime-title"><div className="card-icon"><Icon name="folder" /></div><div><h2 id="project-title">项目目录</h2><p>新建只接受尚不存在的新目录；打开只读取，不修改任何文件。</p></div></div>
          <span className={`state-badge state-${phase}`}><span className="state-dot" />{labels[phase]}</span>
        </div>
        <div className="project-path-row">
          <label htmlFor="project-path">项目目录</label>
          <input id="project-path" type="text" value={path} spellCheck={false} disabled={busy} placeholder="例如 D:\\Noetome\\我的第一个项目" onChange={(event) => setPath(event.target.value)} />
        </div>
        <div className="runtime-actions">
          <button className="button button-secondary" disabled={busy} onClick={() => void pickDirectory()}>选择目录</button>
          <button className="button button-primary" disabled={busy} onClick={() => void run("project_create")}>{busy ? "处理中…" : "新建项目"}</button>
          <button className="button button-secondary" disabled={busy} onClick={() => void run("project_open")}>打开项目</button>
        </div>
        <p className="settings-status" role="status" aria-live="polite">
          {busy ? "正在调用本地组件。" : summary ? "以下字段直接来自本地 Rust 组件的返回结果。" : "新建会生成 project.json 与 project.sqlite；打开不会创建或修复任何内容。"}
        </p>
        {notice && <p className="settings-status" role="status">{notice}</p>}
        {error && <p className="error-message" role="alert">{error}</p>}
      </section>

      {summary && (
        <section className="command-card" aria-labelledby="project-fields-title">
          <div className="command-heading"><div><h2 id="project-fields-title" tabIndex={-1}>项目状态</h2><p>字段与磁盘上的 project.json 一致。</p></div></div>
          <dl className="runtime-details project-details">
            <div><dt>项目目录</dt><dd>{summary.path}</dd></div>
            <div><dt>项目 ID</dt><dd>{summary.project_id}</dd></div>
            <div><dt>结构版本</dt><dd>{summary.schema_version}</dd></div>
            <div><dt>创建时间</dt><dd>{summary.created_at}</dd></div>
            <div><dt>状态</dt><dd>{summary.state}</dd></div>
            <div><dt>数据库文件</dt><dd>{summary.database}</dd></div>
          </dl>
        </section>
      )}

      <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>本页只新建或只读检查项目根：不导入资料、不登记来源、不调用模型、不写 Vault。</p></section>
    </>
  );
}
