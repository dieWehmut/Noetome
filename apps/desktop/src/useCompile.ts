import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectSummary } from "./workbenchTypes";
import { localError } from "./workbenchTypes";
import { compileNeedsRetry } from "./compileTypes";
import type { CompileControl, CompileEvidence, CompileKind, CompileQuery, CompileRun, CompileScope } from "./compileTypes";

const PAGE_SIZE = 20;

/** App-level state keeps a worker alive during navigation; all results bind to their project/run. */
export default function useCompile(project: ProjectSummary | null) {
  const projectPath = project?.path ?? "";
  const scope = `${project?.project_id ?? ""}:${projectPath}`;
  const scopeRef = useRef(scope);
  const generationRef = useRef(0);
  if (scopeRef.current !== scope) { scopeRef.current = scope; generationRef.current += 1; }
  const generation = generationRef.current;
  const [dataGeneration, setDataGeneration] = useState(generation);
  const current = () => generationRef.current === generation;
  const [runs, setRuns] = useState<CompileRun[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [kind, setKind] = useState<CompileKind>("concepts");
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState<CompileQuery | null>(null);
  const [evidence, setEvidence] = useState<CompileEvidence | null>(null);
  const [error, setError] = useState("");
  const [queryError, setQueryError] = useState("");
  const [evidenceError, setEvidenceError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [queryBusy, setQueryBusy] = useState(false);
  const [evidenceBusy, setEvidenceBusy] = useState(false);
  const [runningIds, setRunningIds] = useState<string[]>([]);
  const [exportResult, setExportResult] = useState<unknown>(null);
  const operationLock = useRef<number | null>(null);
  const listLock = useRef<number | null>(null);
  const listRequest = useRef(0);
  const workers = useRef(new Map<string, string>());
  const queryRequest = useRef(0);
  const evidenceRequest = useRef(0);
  const selectedRef = useRef(selectedId);
  selectedRef.current = dataGeneration === generation ? selectedId : null;
  const selected = dataGeneration === generation ? runs.find((run) => run.run_id === selectedId) ?? null : null;

  function acceptRun(run: CompileRun) {
    listRequest.current += 1;
    setRuns((previous) => previous.some((item) => item.run_id === run.run_id) ? previous.map((item) => item.run_id === run.run_id ? run : item) : [run, ...previous]);
  }
  const refresh = useCallback(async () => {
    if (!projectPath || listLock.current === generation) return;
    listLock.current = generation;
    const request = ++listRequest.current;
    try {
      let result = await invoke<CompileRun[]>("compile_list", { project: projectPath });
      if (generationRef.current !== generation || listRequest.current !== request) return;
      const target = selectedRef.current ?? result[0]?.run_id;
      if (target && result.some((run) => run.run_id === target)) {
        // List rows contain cheap persisted counts. Status supplies the current shared budget.
        const status = await invoke<CompileRun>("compile_status", { project: projectPath, runId: target });
        if (status.run_id !== target) throw new Error("DESKTOP_COMPILE_BINDING：状态返回了其他任务。");
        result = result.map((run) => run.run_id === target ? status : run);
      }
      if (generationRef.current !== generation || listRequest.current !== request) return;
      setRuns((previous) => result.map((run) => ({ ...run, budget: run.budget ?? previous.find((item) => item.run_id === run.run_id)?.budget ?? null })));
      setSelectedId((id) => result.some((run) => run.run_id === id) ? id : result[0]?.run_id ?? null);
    } catch (cause) { if (generationRef.current === generation) setError(localError(cause)); }
    finally { if (listLock.current === generation) listLock.current = null; }
  }, [projectPath, generation]);
  useEffect(() => {
    setDataGeneration(generation); setRuns([]); setSelectedId(null); setKind("concepts"); setOffset(0); setSearch("");
    setQuery(null); setEvidence(null); setError(""); setQueryError(""); setEvidenceError(""); setNotice("");
    setBusy(null); setQueryBusy(false); setEvidenceBusy(false); setExportResult(null);
    setRunningIds([...workers.current.entries()].filter(([key]) => key.startsWith(`${scope}\0`)).map(([, id]) => id));
    operationLock.current = null;
    void refresh();
    if (!projectPath) return;
    const timer = window.setInterval(() => void refresh(), 2000);
    return () => window.clearInterval(timer);
  }, [projectPath, generation, scope, refresh]);

  const runId = selected?.run_id ?? null;
  const revision = selected?.ir_revision ?? 0;
  const refreshQuery = useCallback(async () => {
    const request = ++queryRequest.current;
    if (!projectPath || !runId) { setQuery(null); setQueryBusy(false); return; }
    setQueryBusy(true);
    try {
      const result = await invoke<CompileQuery>("compile_query", { project: projectPath, runId, kind, offset, limit: PAGE_SIZE, search });
      if (generationRef.current !== generation || queryRequest.current !== request) return;
      if (result.run_id !== runId || result.kind !== kind || result.offset !== offset || !Array.isArray(result.items) || result.items.length > PAGE_SIZE) throw new Error("DESKTOP_COMPILE_BINDING：查询结果与所选任务或范围不符。");
      setQuery(result); setQueryError("");
    } catch (cause) { if (generationRef.current === generation && queryRequest.current === request) setQueryError(localError(cause)); }
    finally { if (generationRef.current === generation && queryRequest.current === request) setQueryBusy(false); }
  }, [projectPath, generation, runId, kind, offset, search]);
  useEffect(() => { setQuery(null); setQueryError(""); void refreshQuery(); return () => { queryRequest.current += 1; }; }, [refreshQuery, revision]);
  useEffect(() => { evidenceRequest.current += 1; setEvidence(null); setEvidenceError(""); setEvidenceBusy(false); setExportResult(null); }, [generation, runId]);

  async function operation(name: string, action: () => Promise<void>) {
    if (!projectPath || operationLock.current === generation) return;
    operationLock.current = generation; setBusy(name); setError(""); setNotice("");
    try { await action(); } catch (cause) { if (current()) setError(localError(cause)); }
    finally { if (operationLock.current === generation) operationLock.current = null; if (current()) setBusy(null); }
  }
  function create(sourceIds: string[], selection: CompileScope | null = null) {
    return operation("create", async () => {
      const result = await invoke<CompileRun>("compile_create", { project: projectPath, sourceIds, scope: selection });
      if (!current()) return;
      acceptRun(result); setSelectedId(result.run_id); setOffset(0); setSearch(""); setNotice(selection ? "所选物理页范围已固定保存。点击开始后分批整理；更改范围请建立新任务。" : "整本任务已保存。开始后会分批整理全部物理页，缺失页面仍显示问题。");
    });
  }
  async function start(id: string) {
    const key = `${scope}\0${id}`;
    if (!projectPath || workers.current.has(key)) return;
    if (compileNeedsRetry(runs.find((run) => run.run_id === id) ?? null)) {
      setError("请先检查失败原因并选择“重试失败片段”，再开始整理。");
      return;
    }
    workers.current.set(key, id); setRunningIds((previous) => [...previous.filter((item) => item !== id), id]); setError("");
    try {
      const result = await invoke<CompileRun>("compile_run", { project: projectPath, runId: id });
      if (current()) { if (result.run_id !== id) throw new Error("DESKTOP_COMPILE_BINDING：worker返回了其他任务。"); acceptRun(result); }
    } catch (cause) { if (current()) setError(localError(cause)); }
    finally { workers.current.delete(key); if (current()) { setRunningIds((previous) => previous.filter((item) => item !== id)); void refresh(); } }
  }
  function control(action: CompileControl) {
    const id = selected?.run_id;
    return operation(action, async () => {
      if (!id) return;
      const result = await invoke<CompileRun>("compile_control", { project: projectPath, runId: id, action });
      if (current()) {
        if (result.run_id !== id) throw new Error("DESKTOP_COMPILE_BINDING：控制返回了其他任务。");
        acceptRun(result);
        setNotice({
          pause: "暂停要求已保存；正在停止请求与新调度。已确认进度保留。",
          cancel: "取消要求已保存；已确认知识保留，可查询与导出。",
          retry: "可重试的失败片段已重新排队；点击“开始整理”或“恢复整理”才发送请求。旧请求的未知用量预留继续保留。",
        }[action]);
      }
    });
  }
  async function showEvidence(id: string) {
    const targetRun = selected?.run_id;
    if (!targetRun) return;
    const request = ++evidenceRequest.current; setEvidence(null); setEvidenceError(""); setEvidenceBusy(true);
    try {
      const result = await invoke<CompileEvidence>("compile_evidence", { project: projectPath, runId: targetRun, evidenceId: id });
      if (!current() || selectedRef.current !== targetRun || evidenceRequest.current !== request) return;
      if (result.evidence?.evidence_id !== id || !result.page || result.evidence.page_number !== result.page.page_number || result.page.source_id !== undefined && result.evidence.source_id !== result.page.source_id) throw new Error("DESKTOP_COMPILE_BINDING：来源证据身份不符。");
      setEvidence(result);
    } catch (cause) { if (current() && selectedRef.current === targetRun && evidenceRequest.current === request) setEvidenceError(localError(cause)); }
    finally { if (current() && evidenceRequest.current === request) setEvidenceBusy(false); }
  }
  function exportNotes(parent: string, name: string) {
    const id = selected?.run_id;
    return operation("export", async () => {
      if (!id) return;
      const result = await invoke<unknown>("compile_export", { project: projectPath, runId: id, parent, name });
      if (current() && selectedRef.current === id) { setExportResult(result); setNotice("未核验笔记已导出到新目录。来源证据绑定固定页面版本。"); }
    });
  }
  function select(id: string) { setSelectedId(id); setOffset(0); setSearch(""); }
  function changeQuery(next: CompileKind, nextSearch: string) { setKind(next); setSearch(nextSearch); setOffset(0); }
  return { runs: dataGeneration === generation ? runs : [], selected, kind, offset, search, query, evidence, error, queryError, evidenceError, notice,
    busy, queryBusy, evidenceBusy, runningIds, exportResult, pageSize: PAGE_SIZE, refresh, refreshQuery, create, start, control, showEvidence, exportNotes, select, changeQuery, setOffset };
}
export type CompileWorkbench = ReturnType<typeof useCompile>;
