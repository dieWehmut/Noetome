import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import Icon from "./AppIcon";

type Phase = "idle" | "running" | "success" | "error";

/** Fields exactly as the Rust command returns them; nothing is recomputed here. */
interface Fact {
  label: string;
  value: string;
}

interface TaskKind {
  id: string;
  label: string;
}

interface DimensionView {
  id: string;
  label: string;
  state: string;
  reason: string;
  counts: Fact[];
}

interface TaskSummary {
  kind: string;
  kind_label: string;
  root: string;
  state: string;
  state_label: string;
  code: string;
  message: string;
  headline: string;
  facts: Fact[];
  dimensions: DimensionView[];
  counts: Fact[];
  record: unknown;
}

const fallbackKinds: TaskKind[] = [
  { id: "run", label: "六单元运行" },
  { id: "research", label: "研究队列" },
  { id: "retrieve", label: "有界检索" },
  { id: "revision", label: "写作反馈修订" },
];

const knownPrefixes = ["RUN_", "RESEARCH_", "RETRIEVAL_", "REVISION_", "PROJECT_", "DESKTOP_"];

function taskErrorText(error: unknown) {
  if (!isTauri()) return "DESKTOP_IPC_ERROR：当前预览没有桌面运行环境。请在已安装的 Noetome 中读取任务状态。";
  const detail = typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : "本地组件没有返回可用结果。";
  // Rust already answers with `CODE：说明`; keep that text verbatim so the code
  // stays greppable and only wrap messages that carry no code at all.
  if (knownPrefixes.some((prefix) => detail.startsWith(prefix) && detail.includes("："))) return detail;
  return `DESKTOP_TASK_ERROR：${detail}`;
}

const dimensionLabels: Record<string, string> = { complete: "完成", incomplete: "未完成", not_started: "未开始", not_run: "未运行", not_requested: "未请求", not_applicable: "不适用" };

function dimensionLabel(state: string) {
  return dimensionLabels[state] ?? state;
}

export default function AppTasks() {
  const [kinds, setKinds] = useState<TaskKind[]>(fallbackKinds);
  const [kind, setKind] = useState("run");
  const [path, setPath] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [summary, setSummary] = useState<TaskSummary | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [showRecord, setShowRecord] = useState(false);
  const busy = phase === "running";
  const trimmed = path.trim();

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    invoke<TaskKind[]>("task_kinds")
      .then((list) => {
        if (!cancelled && Array.isArray(list) && list.length > 0) setKinds(list);
      })
      .catch(() => {
        if (!cancelled) setNotice("没有取到任务类型清单，已退回本地默认的四类。");
      });
    return () => { cancelled = true; };
  }, []);

  async function pickDirectory() {
    if (busy) return;
    setNotice("");
    try {
      const picked = await invoke<string | null>("pick_directory");
      if (picked) setPath(picked);
      else setNotice("没有选择目录，任务根保持不变。");
    } catch (cause) {
      setError(taskErrorText(cause));
      setPhase("error");
    }
  }

  async function read() {
    if (busy) return;
    if (!trimmed) {
      setPhase("error");
      setSummary(null);
      setError("请先输入任务根目录。任务根由 CLI 建立；本页只读取，不会创建或接管任何目录。");
      return;
    }
    setPhase("running");
    setError("");
    setNotice("");
    setSummary(null);
    setShowRecord(false);
    try {
      const result = await invoke<TaskSummary>("task_read", { kind, path: trimmed });
      setSummary(result);
      setPhase("success");
    } catch (cause) {
      setError(taskErrorText(cause));
      setPhase("error");
    }
  }

  const labels = { idle: "未操作", running: "读取中", success: "已读取", error: "已拒绝" };

  return (
    <>
      <div className="page-heading"><div><p className="eyebrow">TASK CENTER</p><h1>查看每一轮任务的真实状态。</h1><p className="page-description">字段、计数与错误码都来自本地 Rust 组件的只读返回；本页不重算、不补全，也不写入任何账本。</p></div></div>

      <section className="runtime-card" aria-labelledby="tasks-title">
        <div className="runtime-card-heading">
          <div className="runtime-title"><div className="card-icon"><Icon name="queue" /></div><div><h2 id="tasks-title">任务根</h2><p>选择任务类型并填入由 CLI 建立的任务根；读取失败会原样显示 CLI 的错误码。</p></div></div>
          <span className={`state-badge state-${phase}`}><span className="state-dot" />{labels[phase]}</span>
        </div>
        <div className="task-kind-row" role="group" aria-label="任务类型">
          {kinds.map((item) => (
            <button key={item.id} className={`task-kind ${kind === item.id ? "is-active" : ""}`} aria-pressed={kind === item.id} disabled={busy} onClick={() => { setKind(item.id); setSummary(null); setPhase("idle"); setError(""); }}>
              {item.label}
            </button>
          ))}
        </div>
        <div className="project-path-row">
          <label htmlFor="task-root">任务根目录</label>
          <input id="task-root" type="text" value={path} spellCheck={false} disabled={busy} placeholder="例如 /home/我/项目/run-root" onChange={(event) => setPath(event.target.value)} />
        </div>
        <div className="runtime-actions">
          <button className="button button-secondary" disabled={busy} onClick={() => void pickDirectory()}>选择目录</button>
          <button className="button button-primary" disabled={busy} onClick={() => void read()}>{busy ? "读取中…" : "读取任务状态"}</button>
        </div>
        <p className="settings-status" role="status" aria-live="polite">
          {busy ? "正在以只读方式打开账本。" : summary ? "以下内容直接来自本地 Rust 组件，未做任何推导。" : "尚未建立的根会显示为「尚未建立」，不会被当作空任务；损坏的根会显示 CLI 返回的实际错误码。"}
        </p>
        {notice && <p className="settings-status" role="status">{notice}</p>}
        {error && <p className="error-message" role="alert">{error}</p>}
      </section>

      {summary && (
        <>
          <section className="command-card" aria-labelledby="task-summary-title">
            <div className="command-heading"><div><h2 id="task-summary-title" tabIndex={-1}>{summary.kind_label}</h2><p>{summary.headline}</p></div><span className={`state-badge state-${summary.state === "present" ? "success" : summary.state === "absent" ? "idle" : "error"}`}><span className="state-dot" />{summary.state_label}</span></div>
            {summary.code && <p className="error-message" role="alert">{summary.message}</p>}
            <dl className="runtime-details task-details">
              <div><dt>任务根</dt><dd>{summary.root}</dd></div>
              {summary.facts.map((fact) => <div key={fact.label}><dt>{fact.label}</dt><dd>{fact.value}</dd></div>)}
            </dl>
          </section>

          <section className="command-card" aria-labelledby="task-dimensions-title">
            <div className="command-heading"><div><h2 id="task-dimensions-title">四维状态</h2><p>四维分开计数：候选、终止理由与缺口不会并进主任务的完成或失败。</p></div></div>
            <div className="task-dimensions">
              {summary.dimensions.map((dimension) => (
                <div key={dimension.id} className="task-dimension">
                  <div className="task-dimension-heading"><span>{dimension.label}</span><span className={`task-dimension-state state-${dimension.state}`}>{dimensionLabel(dimension.state)}</span></div>
                  <p>{dimension.reason}</p>
                  {dimension.counts.length > 0 && <ul>{dimension.counts.map((entry) => <li key={`${dimension.id}-${entry.label}`}><span>{entry.label}</span><span>{entry.value}</span></li>)}</ul>}
                </div>
              ))}
            </div>
            {summary.counts.length > 0 && <ul className="task-counts">{summary.counts.map((entry) => <li key={entry.label}><span>{entry.label}</span><span>{entry.value}</span></li>)}</ul>}
          </section>

          <section className="command-card" aria-labelledby="task-record-title">
            <div className="command-heading"><div><h2 id="task-record-title">原始记录</h2><p>CLI 返回的完整 JSON 原样保留，便于对照。</p></div><div className="command-actions"><button className="button button-secondary" onClick={() => setShowRecord((visible) => !visible)}>{showRecord ? "收起记录" : "展开记录"}</button></div></div>
            {showRecord && <div className="console-panel"><pre aria-label="任务记录 JSON" tabIndex={0}>{JSON.stringify(summary.record, null, 2)}</pre></div>}
          </section>
        </>
      )}

      <section className="scope-note"><span className="scope-marker" aria-hidden="true" /><p>任务中心只读：不调度新请求、不调用模型、不写知识库或 Vault；读取前后不修改任何账本字节。</p></section>
    </>
  );
}
