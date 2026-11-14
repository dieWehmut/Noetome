import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ExtractionResult, ModelStatus, PreviewExport, ProjectSummary, Selection, SelfTestResult, SourceChoices, SourceImport, SourceUnits } from "./workbenchTypes";
import { bindingKey, failureText, localError, recordField, recordText, selectionKey } from "./workbenchTypes";

const initialSource = () => ({
  path: "", importId: "", imported: null as SourceImport | null,
  choices: null as SourceChoices | null, selection: [] as Selection[],
  units: null as SourceUnits | null, consent: false,
  extracted: null as ExtractionResult | null, resultRoot: "",
  exportParent: "", exportName: "待核验笔记", exported: null as PreviewExport | null,
  error: "", notice: "",
});
type SourceState = ReturnType<typeof initialSource>;
type SourceOperation = "pick" | "import" | "choices" | "units" | "extract" | "export" | "selftest";
const LAST_PROJECT = "noetome.last-project.v1";
interface SavedSource { import_id: string; imported_at: string; filename: string; status: string; media_type: string | null }

/** App owns this hook: changing pages never remounts project or workflow state. */
export default function useWorkbench() {
  const [project, setProject] = useState<ProjectSummary | null>(null);
  const projectRef = useRef<ProjectSummary | null>(null);
  const epoch = useRef(0);
  const [projectForm, setProjectForm] = useState({ mode: "create" as "create" | "open", parent: "", name: "我的学习项目", path: "", error: "", notice: "" });
  const [projectBusy, setProjectBusy] = useState(false);
  const projectLock = useRef(false);
  const [source, setSource] = useState(initialSource);
  const [sourceBusy, setSourceBusy] = useState<SourceOperation | null>(null);
  const sourceLock = useRef(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const cancelLock = useRef(false);
  const [model, setModel] = useState<ModelStatus | null>(null);
  const [modelBusy, setModelBusy] = useState(false);
  const modelLock = useRef(false);
  const [modelError, setModelError] = useState("");
  const [restoreConfirmed, setRestoreConfirmed] = useState(false);
  const [selfTest, setSelfTest] = useState<SelfTestResult | null>(null);
  const [selfTestError, setSelfTestError] = useState("");
  const [savedSources, setSavedSources] = useState<SavedSource[]>([]);
  const [savedSourcesBusy, setSavedSourcesBusy] = useState(false);
  const [savedSourcesError, setSavedSourcesError] = useState("");
  const savedSourcesLock = useRef<number | null>(null);

  function acceptProject(next: ProjectSummary) {
    const previous = projectRef.current;
    if (previous?.project_id !== next.project_id || previous.path !== next.path) {
      epoch.current += 1;
      setSource(initialSource());
    }
    projectRef.current = next;
    setProject(next);
    try { localStorage.setItem(LAST_PROJECT, next.path); return true; }
    catch { return false; }
  }
  useEffect(() => {
    if (!isTauri()) return;
    let path: string | null = null;
    try { path = localStorage.getItem(LAST_PROJECT); } catch { return; }
    if (!path) return;
    const savedPath = path;
    let mounted = true;
    projectLock.current = true; setProjectBusy(true);
    setProjectForm((form) => ({ ...form, mode: "open", path: savedPath, notice: "正在恢复上次打开的项目…" }));
    void invoke<ProjectSummary>("project_open", { path: savedPath }).then((next) => {
      if (!mounted) return;
      const remembered = acceptProject(next);
      setProjectForm((form) => ({ ...form, path: next.path, notice: remembered ? "已恢复上次打开的项目，资料记录正在重新读取。" : "项目已恢复，但无法记住路径；下次启动请重新打开项目。" }));
    }).catch((cause) => {
      if (mounted) setProjectForm((form) => ({ ...form, error: `上次的项目暂时无法打开：${localError(cause)}`, notice: "请检查目录是否仍可用，或打开其他项目。" }));
    }).finally(() => {
      if (mounted) { projectLock.current = false; setProjectBusy(false); }
    });
    return () => { mounted = false; };
  }, []);
  async function pickProjectDirectory() {
    if (projectLock.current) return;
    projectLock.current = true; setProjectBusy(true);
    const mode = projectForm.mode;
    try {
      const path = await invoke<string | null>("pick_directory");
      if (path) setProjectForm((form) => ({ ...form, [mode === "create" ? "parent" : "path"]: path, error: "", notice: "" }));
      else setProjectForm((form) => ({ ...form, notice: "没有选择目录。" }));
    } catch (cause) { setProjectForm((form) => ({ ...form, error: localError(cause) })); }
    finally { projectLock.current = false; setProjectBusy(false); }
  }
  async function openProject() {
    if (projectLock.current) return;
    projectLock.current = true; setProjectBusy(true);
    setProjectForm((form) => ({ ...form, error: "", notice: "" }));
    try {
      const next = projectForm.mode === "create"
        ? await invoke<ProjectSummary>("project_create_in_parent", { parent: projectForm.parent, name: projectForm.name })
        : await invoke<ProjectSummary>("project_open", { path: projectForm.path });
      const remembered = acceptProject(next);
      setProjectForm((form) => ({ ...form, path: next.path, notice: remembered ? "当前项目已载入，可以前往资料页。" : "项目已载入，但无法记住路径；下次启动请重新打开项目。" }));
    } catch (cause) { setProjectForm((form) => ({ ...form, error: localError(cause) })); }
    finally { projectLock.current = false; setProjectBusy(false); }
  }
  function editSource(patch: Partial<SourceState>) { setSource((previous) => ({ ...previous, ...patch })); }
  async function refreshSavedSources() {
    const current = projectRef.current;
    const generation = epoch.current;
    if (!current || savedSourcesLock.current === generation) return;
    savedSourcesLock.current = generation; setSavedSourcesBusy(true);
    try {
      const result = await invoke<{ project_id: string; sources: SavedSource[] }>("source_saved", { project: current.path });
      if (epoch.current !== generation) return;
      if (result.project_id !== current.project_id) throw new Error("DESKTOP_SOURCE_BINDING：资料列表与当前项目不符。");
      setSavedSources(result.sources); setSavedSourcesError("");
    } catch (cause) { if (epoch.current === generation) setSavedSourcesError(localError(cause)); }
    finally {
      if (savedSourcesLock.current === generation) savedSourcesLock.current = null;
      if (epoch.current === generation) setSavedSourcesBusy(false);
    }
  }
  useEffect(() => {
    setSavedSources([]); setSavedSourcesError(""); setSavedSourcesBusy(false);
    void refreshSavedSources();
  }, [project?.project_id, project?.path]);
  function chooseSourcePath(path: string) {
    setSource((previous) => path === previous.path ? previous : { ...initialSource(), path, exportParent: previous.exportParent, exportName: previous.exportName });
  }
  async function runSource(operation: SourceOperation, action: (generation: number) => Promise<void>) {
    if (sourceLock.current) return;
    sourceLock.current = true; setSourceBusy(operation);
    const generation = epoch.current;
    editSource({ error: "", notice: "" });
    try { await action(generation); }
    catch (cause) { if (epoch.current === generation) editSource({ error: localError(cause) }); }
    finally {
      sourceLock.current = false; setSourceBusy(null);
      if (operation === "extract") void refreshModel();
    }
  }
  async function loadChoices(projectPath: string, importId: string, generation: number) {
    const choices = await invoke<SourceChoices>("source_choices", { project: projectPath, importId });
    if (generation !== epoch.current) return;
    if (choices.choices.length > 0 && (choices.binding?.project_id !== projectRef.current?.project_id || choices.binding?.import_id !== importId)) throw new Error("DESKTOP_SOURCE_BINDING：资料选择记录与当前项目或导入 ID 不符。");
    editSource({ importId, choices, selection: [], units: null, consent: false, extracted: null, exported: null });
  }
  function pickSource() {
    return runSource("pick", async (generation) => {
      const path = await invoke<string | null>("source_pick");
      if (generation === epoch.current) {
        if (path) chooseSourcePath(path);
        else editSource({ notice: "没有选择资料文件。" });
      }
    });
  }
  function importSource() {
    const current = projectRef.current;
    if (!current || !source.path.trim()) return;
    if (/\.pdf$/i.test(source.path.trim())) { editSource({ error: "新的 PDF 请使用整本 PDF 资料库进行预检和解析。" }); return; }
    return runSource("import", async (generation) => {
      editSource({ imported: null, choices: null, selection: [], units: null, consent: false, extracted: null, resultRoot: "", exported: null });
      const imported = await invoke<SourceImport>("source_import", { project: current.path, path: source.path });
      if (generation !== epoch.current) return;
      if (imported.project) {
        if (imported.project.project_id !== current.project_id || imported.project.path !== current.path) throw new Error("DESKTOP_SOURCE_BINDING：返回的资料不属于当前项目。");
        projectRef.current = imported.project; setProject(imported.project);
      }
      editSource({ imported });
      void refreshSavedSources();
      if (imported.failure) { editSource({ error: failureText(imported.failure), importId: "" }); return; }
      const importId = recordText(imported.record, "import_id");
      if (!importId) throw new Error("DESKTOP_SOURCE_RECORD：返回记录缺少导入 ID，请查看完整记录。");
      editSource({ importId });
      await loadChoices(current.path, importId, generation);
    });
  }
  function restoreSource(importId: string) {
    const current = projectRef.current;
    if (!current || !importId) return;
    return runSource("choices", async (generation) => {
      editSource({ choices: null, units: null, selection: [], consent: false, extracted: null, exported: null, imported: null });
      await loadChoices(current.path, importId, generation);
    });
  }
  function toggleSelection(selection: Selection) {
    if (sourceLock.current) return;
    const key = selectionKey(selection);
    setSource((previous) => {
      const selected = previous.selection.some((item) => selectionKey(item) === key);
      if (!selected && previous.selection.length >= 64) return previous;
      return { ...previous,
      selection: selected
        ? previous.selection.filter((item) => selectionKey(item) !== key)
        : [...previous.selection, selection],
      units: null, consent: false, extracted: null, resultRoot: "", exported: null,
    }; });
  }
  function selectAvailable() {
    if (sourceLock.current) return;
    const selection: Selection[] = [];
    const keys = new Set<string>();
    let bytes = 0;
    for (const choice of source.choices?.choices ?? []) {
      const key = selectionKey(choice.selection);
      if (selection.length >= 64) break;
      if (!choice.selectable || choice.source_status === "failed" || !Number.isInteger(choice.byte_length) || choice.byte_length <= 0 || bytes + choice.byte_length > 65536 || keys.has(key)) continue;
      selection.push(choice.selection); keys.add(key); bytes += choice.byte_length;
    }
    editSource({ selection, units: null, consent: false, extracted: null, resultRoot: "", exported: null,
      notice: `已选 ${selection.length} 个可用片段，共 ${bytes.toLocaleString()} 字节；最多 64 项和 64 KiB。` });
  }
  function clearSelection() {
    if (!sourceLock.current) editSource({ selection: [], units: null, consent: false, extracted: null, resultRoot: "", exported: null, notice: "已清空选择，请重新核对原文。" });
  }
  function inspectUnits() {
    const current = projectRef.current;
    if (!current || !source.selection.length) return;
    return runSource("units", async (generation) => {
      editSource({ units: null, consent: false, extracted: null, exported: null });
      const units = await invoke<SourceUnits>("source_units", { project: current.path, importId: source.importId, selection: source.selection });
      if (generation !== epoch.current) return;
      if (!source.choices?.binding || bindingKey(units.binding) !== bindingKey(source.choices.binding)) throw new Error("DESKTOP_SOURCE_BINDING：返回的原文与所选资料版本不符，未允许发送模型。");
      const validUnits = units.units.length === source.selection.length && units.units.every((unit, index) => {
        const selected = source.selection[index];
        const locator = unit.locator;
        return unit.source_status !== "failed" && typeof unit.text === "string" && unit.text.length > 0
          && Number.isInteger(unit.start_byte) && Number.isInteger(unit.end_byte) && unit.start_byte >= 0 && unit.end_byte > unit.start_byte
          && new TextEncoder().encode(unit.text).byteLength === unit.end_byte - unit.start_byte
          && (selected.kind === "text" ? locator.kind === "text" && locator.ordinal === selected.ordinal
            : locator.kind === "pdf" && locator.page_index === selected.page_index && locator.region_id === selected.region_id);
      });
      if (!validUnits || units.units.length > 64 || units.units.reduce((size, unit) => size + new TextEncoder().encode(unit.text).byteLength, 0) > 65536) throw new Error("DESKTOP_SOURCE_UNITS：返回的原文不完整、超限或与所选片段不符，未允许发送模型。");
      editSource({ units });
    });
  }
  function extractSource() {
    const current = projectRef.current;
    if (!current || !source.units?.units.length || !source.consent || !model?.ready || modelLock.current || source.choices?.source_status === "failed") return;
    return runSource("extract", async (generation) => {
      editSource({ extracted: null, exported: null, resultRoot: "" });
      const extracted = await invoke<ExtractionResult>("source_extract", { project: current.path, importId: source.importId, selection: source.selection, confirmedUnits: source.units });
      if (generation !== epoch.current) return;
      if (!extracted.failure && (recordText(extracted.record, "status") !== "candidates_validated" || bindingKey(recordField(extracted.record, "binding")) !== bindingKey(source.choices?.binding))) throw new Error("DESKTOP_SOURCE_BINDING：候选状态或来源版本不符，未允许导出。");
      editSource({ extracted, resultRoot: extracted.failure ? "" : extracted.output_root,
        error: extracted.failure ? failureText(extracted.failure) : "" });
    });
  }
  function exportPreview() {
    const current = projectRef.current;
    if (!current || !source.choices || source.choices.source_status === "failed" || !source.resultRoot.trim()) return;
    return runSource("export", async (generation) => {
      editSource({ exported: null });
      const exported = await invoke<PreviewExport>("source_preview_export", { project: current.path, importId: source.importId, resultRoot: source.resultRoot, parent: source.exportParent, name: source.exportName });
      if (generation !== epoch.current) return;
      if (exported.producer !== "extraction_preview" || exported.status !== "pending") throw new Error("DESKTOP_PREVIEW_STATUS：返回的笔记不是待核验预览。");
      editSource({ exported });
    });
  }
  function pickExportParent() {
    return runSource("pick", async (generation) => {
      const path = await invoke<string | null>("pick_directory");
      if (generation === epoch.current && path) editSource({ exportParent: path });
    });
  }
  async function cancelSource() {
    if (cancelLock.current || !sourceLock.current) return;
    cancelLock.current = true; setCancelBusy(true);
    const generation = epoch.current;
    try {
      const cancelled = await invoke<boolean>("source_cancel");
      if (generation === epoch.current) editSource({ notice: cancelled ? "已请求取消，正在等待本地操作返回。" : "当前没有可取消的本地操作。" });
    } catch (cause) { if (generation === epoch.current) editSource({ error: localError(cause) }); }
    finally { cancelLock.current = false; setCancelBusy(false); }
  }
  async function modelCommand(command: "model_status" | "model_credential_store" | "model_budget_restore") {
    if (modelLock.current) return;
    if (command !== "model_status" && sourceLock.current) return;
    if (command === "model_budget_restore" && (!restoreConfirmed || model?.ledger_state !== "missing")) return;
    modelLock.current = true; setModelBusy(true); setModelError("");
    try {
      const status = await invoke<ModelStatus>(command, command === "model_budget_restore" ? { confirmed: true } : undefined);
      setModel(status); setRestoreConfirmed(false);
    } catch (cause) { setModel(null); setModelError(localError(cause)); }
    finally { modelLock.current = false; setModelBusy(false); }
  }
  function refreshModel() { return modelCommand("model_status"); }
  useEffect(() => { if (isTauri()) void refreshModel(); }, []);
  function runSelfTest() {
    return runSource("selftest", async () => {
      setSelfTest(null); setSelfTestError("");
      try {
        const result = await invoke<SelfTestResult>("desktop_self_test");
        setSelfTest(result);
        if (result.failure) setSelfTestError(failureText(result.failure));
      } catch (cause) { setSelfTestError(localError(cause)); }
    });
  }
  return { project, projectForm, setProjectForm, projectBusy, openProject, pickProjectDirectory,
    source, sourceBusy, editSource, chooseSourcePath, pickSource, importSource, restoreSource, toggleSelection, selectAvailable, clearSelection, inspectUnits, extractSource, exportPreview, pickExportParent, cancelSource, cancelBusy,
    savedSources, savedSourcesBusy, savedSourcesError, refreshSavedSources,
    model, modelBusy, modelError, refreshModel, modelCommand, restoreConfirmed, setRestoreConfirmed,
    selfTest, selfTestError, runSelfTest };
}
export type Workbench = ReturnType<typeof useWorkbench>;
