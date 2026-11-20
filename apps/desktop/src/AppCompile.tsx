import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CompileWorkbench } from "./useCompile";
import type { CorpusWorkbench } from "./useCorpus";
import type { ModelStatus } from "./workbenchTypes";
import { localError } from "./workbenchTypes";
import { compileNeedsRetry, compileStatusLabel, evidenceIds, isCompileActive, parseCompileRanges, stringList, textField } from "./compileTypes";
import type { CompileScope } from "./compileTypes";
import { PagePreview } from "./AppCorpus";

export default function AppCompile({ compile: c, corpus, hasProject, model, onProject, onSources, onModel }: {
  compile: CompileWorkbench; corpus: CorpusWorkbench; hasProject: boolean; model: ModelStatus | null;
  onProject: () => void; onSources: () => void; onModel: () => void;
}) {
  const [sourceIds, setSourceIds] = useState<string[]>([]);
  const [scopeMode, setScopeMode] = useState<"all" | "ranges">("all");
  const [rangeTexts, setRangeTexts] = useState<Record<string, string>>({});
  const [searchText, setSearchText] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [formError, setFormError] = useState("");
  const run = c.selected;
  const active = isCompileActive(run) || !!run && c.runningIds.includes(run.run_id);
  const eligible = corpus.sources.filter((source) => !!source.content_sha256 && !!source.page_count && !["acquiring", "interrupted"].includes(source.status));
  useEffect(() => { setSourceIds((ids) => ids.filter((id) => eligible.some((source) => source.source_id === id))); }, [corpus.sources]);
  useEffect(() => { setParent(""); setName(""); setFormError(""); setSearchText(""); }, [run?.run_id, hasProject]);
  const selectedSources = eligible.filter((source) => sourceIds.includes(source.source_id));
  const fullPages = selectedSources.reduce((sum, source) => sum + (source.page_count ?? 0), 0);
  let selection: CompileScope | null = null;
  let rangeError = "";
  if (scopeMode === "ranges" && selectedSources.length > 0) {
    try {
      selection = { schema_version: 1, sources: selectedSources.map((source) => ({ source_id: source.source_id, page_ranges: parseCompileRanges(rangeTexts[source.source_id] ?? "", source.page_count ?? 0) })) };
    } catch (cause) { rangeError = localError(cause); }
  }
  const selectedPages = selection?.sources.reduce((sum, source) => sum + source.page_ranges.reduce((count, range) => count + range.end_page - range.start_page + 1, 0), 0) ?? (scopeMode === "all" ? fullPages : 0);
  const budget = run?.budget;
  const terminal = run?.status === "cancelled";
  const needsRetry = !terminal && compileNeedsRetry(run);
  async function pickParent() {
    try { const value = await invoke<string | null>("pick_directory"); if (value) setParent(value); }
    catch (cause) { setFormError(localError(cause)); }
  }
  return <>
    <div className="page-heading"><div><p className="eyebrow">SOURCE COMPILATION</p><h1>整理资料与章节。</h1><p className="page-description">选择整本或物理页范围，自动分批整理为概念、知识原子和来源证据。所有生成知识都等待独立核验。</p></div></div>
    {!hasProject ? <section className="runtime-card"><h2>先选择一个项目</h2><p>整理任务、已确认进度和未核验知识保存在项目中。</p><button className="button button-primary" onClick={onProject}>建立或打开项目</button></section> : <>
      <section className="runtime-card" aria-labelledby="compile-create-title">
        <div className="command-heading"><div><h2 id="compile-create-title">选择资料与整理范围</h2><p>物理页从 PDF 第一页起计，包含首尾页，与书内印刷页码可能不同。任务创建后范围固定；更改范围请建立新任务。</p></div><button className="button button-secondary" onClick={onSources}>查看或导入资料</button></div>
        {eligible.length === 0 && <p className="workflow-note">资料库尚无保存完成的 PDF。请先导入并保存原件。</p>}
        <div className="workflow-tabs" role="radiogroup" aria-label="整理范围"><label><input type="radio" name="compile-scope" value="all" checked={scopeMode === "all"} disabled={c.busy !== null} onChange={() => setScopeMode("all")} /> 整本资料</label><label><input type="radio" name="compile-scope" value="ranges" checked={scopeMode === "ranges"} disabled={c.busy !== null} onChange={() => setScopeMode("ranges")} /> 物理页范围</label></div>
        <div className="corpus-library" role="group" aria-label="选择整理资料">{eligible.map((source) => <div key={source.source_id} className="corpus-source"><label><input type="checkbox" checked={sourceIds.includes(source.source_id)} disabled={c.busy !== null} onChange={(event) => setSourceIds((ids) => event.target.checked ? [...ids, source.source_id] : ids.filter((id) => id !== source.source_id))} /> <strong>{source.display_name}</strong></label><span>原书 {source.page_count?.toLocaleString()} 页 · 全书待识别 {source.needs_ocr_pages.toLocaleString()} 页 · 全书失败 {source.failed_pages.toLocaleString()} 页</span>{scopeMode === "ranges" && sourceIds.includes(source.source_id) && <div className="project-path-row"><label htmlFor={`compile-range-${source.source_id}`}>物理页范围（一基，含端点）</label><input id={`compile-range-${source.source_id}`} value={rangeTexts[source.source_id] ?? ""} placeholder="例如：34-56 或 34-40, 45-56" disabled={c.busy !== null} onChange={(event) => setRangeTexts((previous) => ({ ...previous, [source.source_id]: event.target.value }))} /></div>}</div>)}</div>
        {rangeError && <p className="error-message" role="alert">{rangeError}</p>}
        <p className="workflow-note">已选择 {sourceIds.length} 份资料，原书共 {fullPages.toLocaleString()} 个物理页。{!rangeError && <>本次整理 {selectedPages.toLocaleString()} 页，范围外 {(fullPages - selectedPages).toLocaleString()} 页。</>}建立任务后，点击“开始整理”才发送模型请求。</p>
        <div className="runtime-actions"><button className="button button-primary" disabled={c.busy !== null || sourceIds.length === 0 || !!rangeError} onClick={() => void c.create(sourceIds, selection)}>{c.busy === "create" ? "正在保存固定范围…" : scopeMode === "all" ? "建立整本任务" : "建立页范围任务"}</button><button className="button button-secondary" onClick={onModel}>模型与累计预算</button></div>
      </section>
      <section className="command-card" aria-labelledby="compile-runs-title">
        <div className="command-heading"><div><h2 id="compile-runs-title">已保存的整理任务</h2><p>重新打开项目后可以继续，已确认的单元不会重复发送。</p></div><button className="button button-secondary" onClick={() => void c.refresh()}>刷新任务</button></div>
        {c.runs.length === 0 && <p className="workflow-note">尚未建立整理任务。</p>}
        <div className="corpus-library" role="group" aria-label="整理任务">{c.runs.map((item) => <button className={`corpus-source ${run?.run_id === item.run_id ? "is-selected" : ""}`} aria-pressed={run?.run_id === item.run_id} key={item.run_id} onClick={() => c.select(item.run_id)}><strong>{item.sources.map((source) => source.display_name).join("、")}</strong><span>{compileStatusLabel[item.status] ?? item.status} · 所选 {item.selected_pages.toLocaleString()} / 原书 {item.physical_pages.toLocaleString()} 页</span><span>{item.units_confirmed.toLocaleString()} / {item.units_total.toLocaleString()} 单元已确认 · {item.atoms.toLocaleString()} 个未核验原子</span></button>)}</div>
      </section>
      {run && <>
        <section className="command-card" aria-labelledby="compile-progress-title">
          <div className="command-heading"><div><h2 id="compile-progress-title">{compileStatusLabel[run.status] ?? run.status}</h2><p>未核验知识 · {["not_run", "not_measured"].includes(run.scientific_quality) ? "科学质量尚未测量" : run.scientific_quality} · 任务 {run.run_id}</p></div><span className="small-badge">{run.out_of_scope_pages > 0 ? "固定物理页范围" : "整本资料"}</span></div>
          <dl className="runtime-details model-details"><div><dt>原书总页数</dt><dd>{run.physical_pages.toLocaleString()} 页</dd></div><div><dt>所选页数</dt><dd>{run.selected_pages.toLocaleString()} 页</dd></div><div><dt>范围外页数</dt><dd>{run.out_of_scope_pages.toLocaleString()} 页</dd></div><div><dt>已尝试准备</dt><dd>{run.prepared_pages.toLocaleString()} 页</dd></div><div><dt>尚未准备</dt><dd>{run.unprepared_pages.toLocaleString()} 页</dd></div><div><dt>范围内失败页</dt><dd>{run.failed_pages.toLocaleString()} 页</dd></div><div><dt>已确认单元</dt><dd>{run.units_confirmed.toLocaleString()} / {run.units_total.toLocaleString()}</dd></div><div><dt>单元失败</dt><dd>{run.units_failed.toLocaleString()}</dd></div><div><dt>待 OCR 页</dt><dd>{run.pending_ocr_pages.toLocaleString()}</dd></div><div><dt>未解决页面</dt><dd>{run.unresolved_pages.toLocaleString()}</dd></div><div><dt>概念 / 原子 / 证据</dt><dd>{run.concepts.toLocaleString()} / {run.atoms.toLocaleString()} / {run.evidence.toLocaleString()}</dd></div><div><dt>本任务尝试次数</dt><dd>{run.attempts.toLocaleString()}</dd></div></dl>
          <progress aria-label="所选范围单元进度" value={run.units_confirmed} max={Math.max(1, run.units_total)} />
          <p className="workflow-note">已尝试准备包含准备失败页，可与失败页重叠；准备、失败、待 OCR 和未解决页均只统计所选范围。空白页和待识别页不发送空内容；暂停会停止当前请求与后续调度，已实际收到的回执继续保存。</p>
          <ul className="workflow-note">{run.sources.map((source) => <li key={source.source_id}>{source.display_name}：{source.page_ranges?.map((range) => range.start_page === range.end_page ? `${range.start_page}` : `${range.start_page}–${range.end_page}`).join("、") ?? `1–${source.physical_pages}`}（物理页，含端点）</li>)}</ul>
          {budget ? <dl className="runtime-details model-details"><div><dt>累计已核算 Token</dt><dd>{budget.accounted_tokens.toLocaleString()}</dd></div><div><dt>待核算预留 Token</dt><dd>{budget.pending_reserved_tokens.toLocaleString()}</dd></div><div><dt>当前可用 Token</dt><dd>{budget.remaining_tokens.toLocaleString()}</dd></div><div><dt>累计模型尝试</dt><dd>{budget.model_attempts.toLocaleString()}</dd></div></dl> : <p className="workflow-note">累计预算尚不可读取。开始时由生产服务核对权威账本，缺失或未知用量不会当作零用量。</p>}
          {!!budget?.pending_reserved_tokens && <p className="workflow-note">存在尚未核算的请求预留，余额已扣除该部分。请求中断不自动释放未知用量。</p>}
          {run.error && <p className="error-message" role="alert">{run.error.code}：{run.error.message}</p>}
          {needsRetry && <p className="workflow-note">任务仍保留错误，尚不能继续整理。请先检查上方失败原因，再选择“重试失败片段”，然后开始整理。</p>}
          {(needsRetry || run.units_failed > 0) && !terminal && <p className="workflow-note">每个失败片段最多发送第二次尝试；达到上限的片段继续保留失败记录。已确认知识不重发，旧请求的未知用量预留不释放。</p>}
          <div className="runtime-actions">{!terminal && <button className="button button-primary" disabled={active || needsRetry || c.busy !== null || !model?.ready || !!budget?.stopped} onClick={() => void c.start(run.run_id)}>{active ? "正在自动分批整理…" : run.units_confirmed > 0 || run.status === "paused" ? "恢复整理" : "开始整理"}</button>}{!terminal && (needsRetry || run.units_failed > 0) && <button className="button button-secondary" disabled={active || c.busy !== null} onClick={() => void c.control("retry")}>{c.busy === "retry" ? "正在重新排队失败片段…" : "重试失败片段"}</button>}<button className="button button-secondary" disabled={!active || c.busy !== null} onClick={() => void c.control("pause")}>{c.busy === "pause" ? "正在保存暂停要求…" : "暂停整理"}</button><button className="button button-secondary" disabled={terminal || c.busy !== null} onClick={() => void c.control("cancel")}>取消任务</button></div>
          {!model?.ready && !terminal && <p className="workflow-note">请在“模型与累计预算”中检查凭据与预算状态，然后开始整理。</p>}
          <details className="workflow-record"><summary>固定资料版本与任务记录</summary><pre tabIndex={0}>{JSON.stringify({ sources: run.sources, status: run.status, stage: run.stage, knowledge_status: run.knowledge_status, scientific_quality: run.scientific_quality, ir_revision: run.ir_revision, created_at: run.created_at }, null, 2)}</pre></details>
        </section>
        <section className="command-card" aria-labelledby="compile-knowledge-title" aria-busy={c.queryBusy}>
          <h2 id="compile-knowledge-title">查询未核验知识</h2>
          <div className="workflow-tabs" role="group" aria-label="知识类型">{(["concepts", "atoms", "evidence"] as const).map((kind) => <button className={`button button-secondary ${c.kind === kind ? "is-selected" : ""}`} aria-pressed={c.kind === kind} key={kind} onClick={() => c.changeQuery(kind, c.search)}>{({ concepts: "概念", atoms: "知识原子", evidence: "来源证据" })[kind]}</button>)}</div>
          <form className="project-path-row" onSubmit={(event) => { event.preventDefault(); c.changeQuery(c.kind, searchText.trim()); }}><label htmlFor="compile-search">搜索当前类型</label><input id="compile-search" value={searchText} onChange={(event) => setSearchText(event.target.value)} /><button className="button button-secondary" type="submit">查询</button></form>
          {c.queryError && <p className="error-message" role="alert">{c.queryError}</p>}
          {c.query && <><p className="workflow-note">{c.query.total === 0 ? "没有匹配结果。" : `显示 ${c.offset + 1}–${c.offset + c.query.items.length} / ${c.query.total.toLocaleString()} 项。`}</p>{c.query.items.map((row, index) => <article className="corpus-page" key={textField(row, "concept_id") || textField(row, "atom_id") || textField(row, "evidence_id") || index}>
            {c.kind === "concepts" ? <><h3>{textField(row, "canonical_name")}</h3><p>{stringList(row, "aliases").join("、") || "没有别名"}</p><p className="workflow-note">概念身份与归并结果仍需核对。</p></> : c.kind === "atoms" ? <><h3>知识原子 · 未核验</h3><p style={{ whiteSpace: "pre-wrap" }}>{textField(row, "content")}</p>{stringList(row, "qualifiers").length > 0 && <p>适用条件：{stringList(row, "qualifiers").join("；")}</p>}<div className="runtime-actions">{evidenceIds(row).map((id) => <button className="button button-secondary" key={id} onClick={() => void c.showEvidence(id)}>查看来源证据 {id}</button>)}</div></> : <><h3>物理第 {Number(row.page_number)} 页 · 固定版本证据</h3><blockquote style={{ whiteSpace: "pre-wrap" }}>{textField(row, "quote")}</blockquote><button className="button button-secondary" onClick={() => void c.showEvidence(textField(row, "evidence_id"))}>跳转到来源原页</button></>}
            <details className="workflow-record"><summary>完整结构记录</summary><pre tabIndex={0}>{JSON.stringify(row, null, 2)}</pre></details>
          </article>)}<div className="corpus-pagination" aria-label="知识查询分页"><button className="button button-secondary" disabled={c.queryBusy || c.offset === 0} onClick={() => c.setOffset(Math.max(0, c.offset - c.pageSize))}>上一页结果</button><button className="button button-secondary" disabled={c.queryBusy || c.offset + c.pageSize >= c.query.total} onClick={() => c.setOffset(c.offset + c.pageSize)}>下一页结果</button></div></>}
        </section>
        {(c.evidenceBusy || c.evidence || c.evidenceError) && <section className="command-card" aria-labelledby="compile-evidence-title" aria-busy={c.evidenceBusy}><h2 id="compile-evidence-title">来源证据 · 抽取时的固定页面</h2><p className="workflow-note">页面正文与资产按保存的 SHA256 重新校验。后续 OCR 产生的新版本不会替换这条证据。</p>{c.evidenceBusy && <p>正在校验固定页版本…</p>}{c.evidenceError && <p className="error-message" role="alert">{c.evidenceError}</p>}{c.evidence && <><blockquote style={{ whiteSpace: "pre-wrap" }}>{textField(c.evidence.evidence, "quote")}</blockquote><PagePreview key={`${textField(c.evidence.evidence, "evidence_id")}:${textField(c.evidence.evidence, "page_sha256")}`} page={c.evidence.page} fixedSha={textField(c.evidence.evidence, "page_sha256")} /><details className="workflow-record"><summary>固定来源定位</summary><pre tabIndex={0}>{JSON.stringify(c.evidence.evidence, null, 2)}</pre></details></>}</section>}
        <section className="command-card" aria-labelledby="compile-export-title"><h2 id="compile-export-title">导出未核验笔记</h2><p>导出包含概念、原子、来源证据和未解决问题。请选择父目录并填写一个尚不存在的新文件夹名。</p><div className="project-path-row"><label htmlFor="compile-export-parent">导出父目录</label><input id="compile-export-parent" value={parent} onChange={(event) => setParent(event.target.value)} /><button className="button button-secondary" disabled={c.busy !== null} onClick={() => void pickParent()}>选择父目录</button></div><div className="project-path-row"><label htmlFor="compile-export-name">新文件夹名</label><input id="compile-export-name" value={name} placeholder="例如：未核验笔记-01" onChange={(event) => setName(event.target.value)} /></div><button className="button button-primary" disabled={c.busy !== null || !parent.trim() || !name.trim()} onClick={() => void c.exportNotes(parent.trim(), name.trim())}>{c.busy === "export" ? "正在校验并导出…" : "导出到新目录"}</button>{formError && <p className="error-message" role="alert">{formError}</p>}{c.exportResult != null && <details className="workflow-record"><summary>导出结果</summary><pre tabIndex={0}>{JSON.stringify(c.exportResult, null, 2)}</pre></details>}</section>
      </>}
    </>}
    {c.error && <p className="error-message" role="alert">{c.error}</p>}{c.notice && <p className="workflow-note" role="status">{c.notice}</p>}
  </>;
}
