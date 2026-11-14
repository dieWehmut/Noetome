import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CorpusPages, CorpusPreflight, CorpusSource } from "./corpusTypes";
import type { ProjectSummary } from "./workbenchTypes";
import { localError } from "./workbenchTypes";

const PAGE_SIZE = 5;
type InputOperation = "pick" | "preflight" | "import" | null;

/** Kept at App level. Navigation does not interrupt a persisted parser job. */
export default function useCorpus(project: ProjectSummary | null) {
  const projectPath = project?.path ?? "";
  const scope = `${project?.project_id ?? ""}:${projectPath}`;
  const scopeRef = useRef(scope);
  const generationRef = useRef(0);
  if (scopeRef.current !== scope) { scopeRef.current = scope; generationRef.current += 1; }
  const generation = generationRef.current;
  const isCurrent = () => generationRef.current === generation;
  const [dataGeneration, setDataGeneration] = useState(generation);
  const dataIsCurrent = dataGeneration === generation;
  const [path, setPath] = useState("");
  const [preflight, setPreflight] = useState<CorpusPreflight | null>(null);
  const [sources, setSources] = useState<CorpusSource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [pages, setPages] = useState<CorpusPages | null>(null);
  const [inputBusy, setInputBusy] = useState<InputOperation>(null);
  const [listBusy, setListBusy] = useState(false);
  const [pagesBusy, setPagesBusy] = useState(false);
  const [error, setError] = useState("");
  const [listError, setListError] = useState("");
  const [pagesError, setPagesError] = useState("");
  const [notice, setNotice] = useState("");
  const [runningIds, setRunningIds] = useState<string[]>([]);
  const [pauseIds, setPauseIds] = useState<string[]>([]);
  const inputLock = useRef<number | null>(null);
  const listLock = useRef<number | null>(null);
  const processLocks = useRef(new Map<string, { scope: string; id: string }>());
  const pageRequest = useRef(0);
  const pageSourceId = dataIsCurrent ? selectedId : null;
  const selected = dataIsCurrent ? sources.find((source) => source.source_id === selectedId) ?? null : null;

  function acceptSource(source: CorpusSource) {
    setSources((previous) => {
      const index = previous.findIndex((item) => item.source_id === source.source_id);
      return index < 0 ? [source, ...previous] : previous.map((item, at) => at === index ? source : item);
    });
  }

  const refreshList = useCallback(async () => {
    if (!projectPath || listLock.current === generation) return;
    listLock.current = generation;
    setListBusy(true);
    try {
      const result = await invoke<CorpusSource[]>("corpus_list", { project: projectPath });
      if (generationRef.current !== generation) return;
      setSources(result);
      setListError("");
      setSelectedId((previous) => result.some((source) => source.source_id === previous) ? previous : result[0]?.source_id ?? null);
      setPauseIds((previous) => previous.filter((id) => processLocks.current.has(`${scope}:${id}`) || result.some((source) => source.source_id === id && source.status === "running")));
    } catch (cause) {
      if (generationRef.current === generation) setListError(localError(cause));
    } finally {
      if (listLock.current === generation) listLock.current = null;
      if (generationRef.current === generation) setListBusy(false);
    }
  }, [projectPath, scope, generation]);

  useEffect(() => {
    setDataGeneration(generation);
    setPath(""); setPreflight(null); setSources([]); setSelectedId(null); setOffset(0); setPages(null);
    setError(""); setListError(""); setPagesError(""); setNotice(""); setInputBusy(null);
    setRunningIds([...processLocks.current.values()].filter((task) => task.scope === scope).map((task) => task.id));
    setPauseIds([]); setPagesBusy(false); setListBusy(false);
    inputLock.current = null;
    if (!projectPath) return;
    void refreshList();
    const timer = window.setInterval(() => void refreshList(), 2500);
    return () => window.clearInterval(timer);
  }, [projectPath, scope, refreshList]);

  const refreshPages = useCallback(async () => {
    const request = ++pageRequest.current;
    if (!projectPath || !pageSourceId) { setPages(null); setPagesBusy(false); return; }
    setPagesBusy(true);
    try {
      const result = await invoke<CorpusPages>("corpus_pages", { project: projectPath, sourceId: pageSourceId, offset, limit: PAGE_SIZE });
      if (generationRef.current !== generation || pageRequest.current !== request) return;
      if (result.source_id !== pageSourceId || result.offset !== offset || result.pages.length > PAGE_SIZE) throw new Error("DESKTOP_CORPUS_BINDING：页面记录与所选资料或分页范围不符。");
      setPages(result); setPagesError("");
    } catch (cause) {
      if (generationRef.current === generation && pageRequest.current === request) setPagesError(localError(cause));
    } finally {
      if (generationRef.current === generation && pageRequest.current === request) setPagesBusy(false);
    }
  }, [projectPath, generation, pageSourceId, offset]);

  useEffect(() => {
    setPages(null); setPagesError("");
  }, [projectPath, generation, selectedId, offset]);
  useEffect(() => {
    void refreshPages();
    return () => { pageRequest.current += 1; };
  }, [refreshPages, selected?.processed_pages, selected?.status]);

  function selectSource(id: string) { setSelectedId(id); setOffset(0); setError(""); setNotice(""); }
  function choosePath(next: string) { setPath(next); setPreflight(null); setError(""); setNotice(""); }
  async function inspectPath(next: string) {
    if (isCurrent()) setPreflight(null);
    const result = await invoke<CorpusPreflight>("corpus_preflight", { path: next });
    if (isCurrent()) setPreflight(result);
  }
  async function inputOperation(operation: Exclude<InputOperation, null>, action: () => Promise<void>) {
    if (inputLock.current === generation) return;
    inputLock.current = generation; setInputBusy(operation); setError(""); setNotice("");
    try { await action(); }
    catch (cause) { if (isCurrent()) setError(localError(cause)); }
    finally {
      if (inputLock.current === generation) inputLock.current = null;
      if (isCurrent()) setInputBusy(null);
    }
  }
  function pickPdf() {
    return inputOperation("pick", async () => {
      const next = await invoke<string | null>("corpus_pick");
      if (!isCurrent()) return;
      if (!next) { setNotice("没有选择 PDF。"); return; }
      choosePath(next); setInputBusy("preflight");
      await inspectPath(next);
    });
  }
  function checkPdf() { return inputOperation("preflight", () => inspectPath(path)); }

  async function processSource(id: string, retry = false) {
    const key = `${scope}:${id}`;
    if (!projectPath || processLocks.current.has(key)) return;
    processLocks.current.set(key, { scope, id });
    if (isCurrent()) {
      setRunningIds((previous) => [...previous, id]); setError(""); setNotice("");
      setPauseIds((previous) => previous.filter((item) => item !== id));
    }
    try {
      const result = await invoke<CorpusSource>(retry ? "corpus_retry" : "corpus_process", { project: projectPath, sourceId: id });
      if (!isCurrent()) return;
      acceptSource(result);
      setNotice(result.status === "paused" ? "已暂停，已保存页面会在继续时复用。" : result.status === "ready" ? "原文解析完成。知识整理尚未开始。" : "本次解析已返回，请查看资料状态和页面问题。");
    } catch (cause) {
      if (isCurrent()) setError(localError(cause));
    } finally {
      processLocks.current.delete(key);
      if (scopeRef.current === scope) {
        setRunningIds((previous) => previous.filter((item) => item !== id));
        setPauseIds((previous) => previous.filter((item) => item !== id));
        if (isCurrent()) void refreshList();
      }
    }
  }
  function importPdf() {
    if (!projectPath || preflight?.status !== "ready") return;
    return inputOperation("import", async () => {
      const result = await invoke<CorpusSource>("corpus_import", { project: projectPath, path });
      if (!isCurrent()) return;
      acceptSource(result); selectSource(result.source_id); setPath(""); setPreflight(null);
      if (result.status === "queued" || result.status === "paused") void processSource(result.source_id);
      else if (result.status === "failed") setError(result.error ? `${result.error.code}：${result.error.message}` : "原件已登记，但解析准备未完成。请查看资料记录。");
      else setNotice("资料已登记，请查看已保存的状态。");
      void refreshList();
    });
  }
  async function pauseSource(id: string) {
    if (!projectPath || pauseIds.includes(id)) return;
    setPauseIds((previous) => [...previous, id]); setError("");
    try {
      const result = await invoke<CorpusSource>("corpus_pause", { project: projectPath, sourceId: id });
      if (!isCurrent()) return;
      acceptSource(result);
      setNotice(result.status === "paused" ? "已暂停，已保存页面会在继续时复用。" : "已请求暂停，正在等待解析停止并保存状态。");
      if (result.status !== "running" && !processLocks.current.has(`${scope}:${id}`)) setPauseIds((previous) => previous.filter((item) => item !== id));
    } catch (cause) {
      if (isCurrent()) {
        setError(localError(cause)); setPauseIds((previous) => previous.filter((item) => item !== id));
      }
    }
  }

  return { path: dataIsCurrent ? path : "", choosePath, preflight: dataIsCurrent ? preflight : null,
    sources: dataIsCurrent ? sources : [], selected, selectSource, pages: dataIsCurrent && pages?.source_id === selectedId && pages.offset === offset ? pages : null, offset, setOffset,
    pageSize: PAGE_SIZE, inputBusy, listBusy, pagesBusy, error, listError, pagesError, notice,
    runningIds, pauseIds, pickPdf, checkPdf, importPdf, processSource, pauseSource, refreshList, refreshPages };
}
export type CorpusWorkbench = ReturnType<typeof useCorpus>;
