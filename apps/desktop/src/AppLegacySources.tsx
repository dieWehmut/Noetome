import Icon from "./AppIcon";
import type { Workbench } from "./useWorkbench";
import { recordField, selectionKey } from "./workbenchTypes";

function RecordDetails({ record }: { record: unknown }) {
  const pdf = recordField(record, "pdf");
  const fields = [
    ["处理状态", recordField(record, "status")], ["知识状态", recordField(record, "knowledge_status")],
    ["科学核验", recordField(record, "scientific_quality")], ["原文段落", recordField(record, "span_count")],
    ["PDF 页数", recordField(pdf, "page_count")], ["PDF 区域", recordField(pdf, "region_count")],
    ["候选原子", recordField(record, "validated_atoms")],
  ].filter(([, value]) => typeof value === "string" || typeof value === "number");
  return <>{fields.length > 0 && <dl className="runtime-details model-details">{fields.map(([label, value]) => <div key={String(label)}><dt>{String(label)}</dt><dd>{String(value)}</dd></div>)}</dl>}<details className="workflow-record"><summary>完整本地记录</summary><pre tabIndex={0}>{JSON.stringify(record, null, 2)}</pre></details></>;
}
export default function AppLegacySources({ workbench: w, onProject, onModel }: { workbench: Workbench; onProject: () => void; onModel: () => void }) {
  const s = w.source;
  const busy = w.sourceBusy !== null;
  const selectedKeys = new Set(s.selection.map(selectionKey));
  const selectedBytes = s.choices?.choices.reduce((sum, choice) => selectedKeys.has(selectionKey(choice.selection)) ? sum + choice.byte_length : sum, 0) ?? 0;
  const selectionWithinLimit = s.selection.length > 0 && s.selection.length <= 64 && selectedBytes <= 65536;
  const sourceFailed = s.imported?.failure != null || s.choices?.source_status === "failed";
  const unitsReady = !!s.units?.units.length && selectionWithinLimit && !sourceFailed;
  const extractionFailed = !!s.extracted?.failure;
  const canExport = !!s.choices && !sourceFailed && !extractionFailed && !!s.resultRoot.trim() && !!s.exportParent.trim() && !!s.exportName.trim();
  return <>
    {!w.project ? <section className="runtime-card"><h2>先选择一个项目</h2><p className="settings-description">资料与处理记录会保存在所选项目中。</p><button className="button button-primary" onClick={onProject}>建立或打开项目<Icon name="arrow" /></button></section> : <>
      <section className="runtime-card" aria-labelledby="source-import-title" aria-busy={busy}>
        <div className="runtime-title"><div className="card-icon"><Icon name="book" /></div><div><h2 id="source-import-title">1. 导入文本或恢复旧资料</h2><p>这里保留 Markdown、TXT 和旧版导入记录的片段处理功能。新的 PDF 请使用“整本 PDF”入口。</p></div></div>
        <div className="project-path-row"><label htmlFor="source-path">文本资料文件</label><input id="source-path" value={s.path} disabled={busy} spellCheck={false} placeholder="例如 D:\资料\我的记录.md" onChange={(event) => w.chooseSourcePath(event.target.value)} /></div>
        <div className="runtime-actions"><button className="button button-secondary" disabled={busy} onClick={() => void w.pickSource()}>选择资料文件</button><button className="button button-primary" disabled={busy || !s.path.trim() || /\.pdf$/i.test(s.path.trim())} onClick={() => void w.importSource()}>{w.sourceBusy === "import" ? "正在导入…" : "导入并检查"}</button></div>
        <p className="workflow-note">文本导入单文件上限 64 MiB；单次模型选择最多 64 个片段、合计 64 KiB。此入口按所选片段生成候选，不是整书自动整理。</p>
        {/\.pdf$/i.test(s.path.trim()) && <p className="error-message" role="alert">新的 PDF 需要在“整本 PDF”中预检和解析。</p>}
        <details className="workflow-record"><summary>恢复已保存的文本与旧版 PDF</summary><div className="runtime-actions"><button className="button button-secondary" disabled={w.savedSourcesBusy} onClick={() => void w.refreshSavedSources()}>{w.savedSourcesBusy ? "正在读取…" : "刷新旧资料列表"}</button></div>
          {w.savedSourcesError && <p className="error-message" role="alert">{w.savedSourcesError}</p>}
          {!w.savedSourcesBusy && w.savedSources.length === 0 && !w.savedSourcesError && <p className="workflow-note">这个项目还没有旧版资料记录。</p>}
          <div className="corpus-library" role="group" aria-label="已保存文本与旧资料">{w.savedSources.map((saved) => <button key={saved.import_id} className={`corpus-source ${s.importId === saved.import_id ? "is-selected" : ""}`} disabled={busy} aria-pressed={s.importId === saved.import_id} onClick={() => void w.restoreSource(saved.import_id)}><strong>{saved.filename || "未命名资料"}</strong><span>{saved.media_type === "application/pdf" ? "旧版 PDF" : "文本资料"} · {saved.status}</span><span>{saved.imported_at}</span></button>)}</div>
        </details>
        {s.imported && <><p className="settings-status" role="status">{s.imported.failure ? "资料导入失败，未进入模型步骤。" : `导入记录：${s.importId || "请查看完整记录"}`}</p><RecordDetails record={s.imported.record} /></>}
      </section>
      {s.choices && <section className="command-card" aria-labelledby="source-choice-title">
        <div className="command-heading"><div><h2 id="source-choice-title">2. 选择要整理的原文</h2><p>{s.choices.filename} · 来源状态：{s.choices.source_status}</p></div><span className="small-badge">{s.selection.length} / 64 项</span></div>
        {s.choices.source_status === "needs_review" && <p className="workflow-note">这份资料需要复核。只有已经确认、带文字的区域才能选择；扫描图像不会自动当作原文文字。</p>}
        {s.choices.choices.length === 0 && <p className="workflow-note">没有可用于模型整理的文字区域。扫描页或无文字资料需先核对文字来源。</p>}
        <div className="runtime-actions"><button className="button button-secondary" disabled={busy || sourceFailed || !s.choices.choices.some((choice) => choice.selectable && choice.byte_length > 0)} onClick={w.selectAvailable}>选择前64个可用片段</button><button className="button button-secondary" disabled={busy || s.selection.length === 0} onClick={w.clearSelection}>清空选择</button></div>
        <fieldset className="source-choices" disabled={busy || sourceFailed}><legend className="visually-hidden">原文区域选择</legend>{s.choices.choices.map((choice) => {
          const key = selectionKey(choice.selection);
          const selected = selectedKeys.has(key);
          return <label className={`source-choice ${!choice.selectable || choice.byte_length <= 0 ? "is-unavailable" : ""}`} key={key}><input type="checkbox" checked={selected} disabled={!choice.selectable || choice.byte_length <= 0 || (!selected && s.selection.length >= 64)} onChange={() => w.toggleSelection(choice.selection)} /><span><strong>{choice.label}</strong><span>{choice.byte_length} 字节 · {choice.source_status}{choice.reason ? ` · ${choice.reason}` : ""}</span></span></label>;
        })}</fieldset>
        <div className="runtime-actions"><button className="button button-secondary" disabled={busy || !selectionWithinLimit || sourceFailed} onClick={() => void w.inspectUnits()}>{w.sourceBusy === "units" ? "正在读取…" : "查看将发送的完整原文"}</button><span role="status">已选 {selectedBytes.toLocaleString()} / 65,536 字节</span></div>
        {selectedBytes > 65536 && <p className="error-message" role="alert">原文超出单次 64 KiB 上限，请减少选择。</p>}
      </section>}
      {s.units && <section className="command-card" aria-labelledby="source-units-title">
        <div className="command-heading"><div><h2 id="source-units-title">3. 核对原文并生成候选</h2><p>以下是本次将发送的完整文字，不包含未选资料。</p></div></div>
        {s.units.units.map((unit, index) => <article className="source-unit" key={unit.unit_id}><h3>所选片段 {index + 1} · {unit.source_status === "ready" ? "原文可用" : unit.source_status === "needs_review" ? "需复核" : unit.source_status}</h3><p>{unit.locator.kind === "text" ? `文本片段 ${unit.locator.ordinal + 1}` : `物理第 ${unit.locator.page_index + 1} 页 · 区域 ${unit.locator.region_id}`} · 原文范围：{unit.start_byte}–{unit.end_byte} 字节</p>{unit.locator.kind === "pdf" && <p>页面坐标范围：横向 {unit.locator.bbox[0]}–{unit.locator.bbox[2]}，纵向 {unit.locator.bbox[1]}–{unit.locator.bbox[3]}</p>}<pre tabIndex={0}>{unit.text}</pre><details className="workflow-record"><summary>完整来源定位</summary><pre>{JSON.stringify(unit.locator, null, 2)}</pre></details></article>)}
        {!w.model?.ready && <p className="workflow-note">{w.model?.message || "模型状态尚未就绪。"} <button className="text-button" onClick={onModel}>前往模型设置<Icon name="arrow" /></button></p>}
        {w.model?.ready && <p className="settings-status">本次服务：{w.model.model} · {w.model.endpoint}</p>}
        <label className="workflow-consent"><input type="checkbox" checked={s.consent} disabled={busy || !unitsReady} onChange={(event) => w.editSource({ consent: event.target.checked })} /><span>我已核对以上原文，同意将这些文字发送给当前模型服务。</span></label>
        <div className="runtime-actions"><button className="button button-primary" disabled={busy || !unitsReady || !s.consent || !w.model?.ready || w.modelBusy} onClick={() => void w.extractSource()}>{w.sourceBusy === "extract" ? "模型正在整理…" : s.extracted && !s.extracted.failure ? "重新生成候选（再次调用模型）" : "生成候选（调用模型）"}</button><span>单次有界调用；候选仍需人工核验。</span></div>
      </section>}
      {s.extracted && <section className="command-card"><div className="command-heading"><div><h2>{s.extracted.failure ? "模型整理失败" : "候选已生成 · 待核验"}</h2><p>{s.extracted.failure ? "失败记录保留，不作为可导出的成功结果。" : "来源与模型候选检查完成，不代表知识已经科学核验。"}</p></div></div><p className="workflow-path">结果目录：{s.extracted.output_root}</p><RecordDetails record={s.extracted.record} /></section>}
      {s.choices && !sourceFailed && <section className="command-card" aria-labelledby="source-export-title">
        <div className="command-heading"><div><h2 id="source-export-title">4. 导出 Obsidian 待核验笔记</h2><p>使用新的测试目录。笔记保留候选、原文证据与定位，状态为 pending。</p></div></div>
        <div className="project-path-row"><label htmlFor="result-root">已保存 API 结果目录</label><input id="result-root" value={s.resultRoot} spellCheck={false} disabled={busy} onChange={(event) => w.editSource({ resultRoot: event.target.value, extracted: null, exported: null })} /><span className="settings-status">已有结果可直接恢复导出，不会再次调用模型。</span></div>
        <div className="project-path-row"><label htmlFor="export-parent">导出父目录</label><input id="export-parent" value={s.exportParent} spellCheck={false} disabled={busy} placeholder="例如 D:\Noetome实测" onChange={(event) => w.editSource({ exportParent: event.target.value })} /></div>
        <div className="project-path-row"><label htmlFor="export-name">新笔记目录名称</label><input id="export-name" value={s.exportName} disabled={busy} onChange={(event) => w.editSource({ exportName: event.target.value })} /></div>
        <div className="runtime-actions"><button className="button button-secondary" disabled={busy} onClick={() => void w.pickExportParent()}>选择导出父目录</button><button className="button button-primary" disabled={busy || !canExport} onClick={() => void w.exportPreview()}>{w.sourceBusy === "export" ? "正在导出…" : "导出待核验笔记"}</button></div>
        {s.exported && <div className="preview-result" role="status"><h3>待核验笔记已导出</h3><p>{s.exported.producer} · {s.exported.status}</p><p className="workflow-path">在 Obsidian 中打开：{s.exported.vault_path}</p><p>概念 {s.exported.counts.concepts} · 原子 {s.exported.counts.atoms} · 证据 {s.exported.counts.evidence}</p><pre tabIndex={0}>{s.exported.markdown}</pre></div>}
      </section>}
    </>}
    {busy && <div className="workflow-operation" role="status"><span>正在执行本地操作，切换页面会保留进度。</span>{(w.sourceBusy === "extract" || w.sourceBusy === "import") && <button className="button button-secondary" disabled={w.cancelBusy} onClick={() => void w.cancelSource()}>{w.cancelBusy ? "正在请求取消…" : "取消当前操作"}</button>}</div>}
    {s.notice && <p className="settings-status" role="status">{s.notice}</p>}
    {s.error && <p className="error-message" role="alert">{s.error}</p>}
  </>;
}
