import { useState } from "react";
import type { CorpusIssue, CorpusPage, CorpusPreflight } from "./corpusTypes";
import { byteSize, corpusPageStatusLabel, corpusStatusLabel } from "./corpusTypes";
import type { CorpusWorkbench } from "./useCorpus";

function Issues({ issues }: { issues: CorpusIssue[] }) {
  return issues.length > 0 && <ul className="corpus-issues">{issues.map((issue, index) => <li key={`${issue.code}-${index}`}><strong>{issue.code}</strong>：{issue.message}</li>)}</ul>;
}

function Preflight({ value }: { value: CorpusPreflight }) {
  const limits = value.limits;
  return <div className="corpus-preflight" aria-label="PDF 预检结果">
    <h3>{value.status === "ready" ? "可以登记并解析" : value.password_required ? "PDF 需要密码" : "PDF 预检未通过"}</h3>
    <dl className="runtime-details model-details">
      <div><dt>原件大小</dt><dd>{byteSize(value.byte_length)}</dd></div>
      <div><dt>物理页数</dt><dd>{value.page_count?.toLocaleString() ?? "尚未确定"}</dd></div>
      <div><dt>密码保护</dt><dd>{value.password_required ? "需要先提供可打开的无密码副本" : value.encrypted ? "已加密，可直接读取" : "没有"}</dd></div>
      <div><dt>扫描文字识别</dt><dd>{limits.ocr_available ? "本机 OCR 可用" : "本机 OCR 尚不可用；扫描页会保留待识别状态"}</dd></div>
    </dl>
    <p className="workflow-note">项目会保存一份 {byteSize(value.byte_length)} 的原件副本，文字、图像及识别结果还需额外磁盘空间。</p>
    <ul className="corpus-limits">
      <li>{limits.max_file_bytes === null ? "单文件未设固定大小上限" : `单文件上限 ${byteSize(limits.max_file_bytes)}`}；{limits.max_pages === null ? "整书未设固定页数上限" : `整书上限 ${limits.max_pages.toLocaleString()} 页`}，实际受可用磁盘与本机资源限制。</li>
      <li>文字解析每批最多 {limits.default_batch_pages} 页，单批等待上限 {limits.batch_timeout_seconds} 秒；单页结构结果上限 {byteSize(limits.max_page_json_bytes)}。超限页会留下错误记录，已保存页面保留。</li>
      {value.ocr?.available && <li>扫描识别使用 {value.ocr.engine} {value.ocr.engine_version}{value.ocr.page_timeout_seconds && `，单页等待上限 ${value.ocr.page_timeout_seconds} 秒`}{value.ocr.render_dpi && `，原页渲染 ${value.ocr.render_dpi} DPI`}。识别结果仍需独立核对。</li>}
      <li>预检只检查部分页面。完整页覆盖、文字质量与问题数在逐页解析后确定。</li>
    </ul>
    <Issues issues={value.issues} />
    {!value.ocr?.available && value.ocr?.message && <p className="workflow-note">{value.ocr.message}</p>}
    {value.ocr?.error && <p className="error-message" role="alert">{value.ocr.error.code}：{value.ocr.error.message}</p>}
    {value.text_sample.length > 0 && <details className="workflow-record"><summary>预检抽样页（{value.text_sample.length} 页）</summary><ul className="corpus-samples">{value.text_sample.map((sample) => <li key={sample.page_number}>物理第 {sample.page_number} 页：{sample.text_characters.toLocaleString()} 个文字字符 · {sample.status}<Issues issues={sample.issues} /></li>)}</ul></details>}
    {value.outline.length > 0 && <details className="workflow-record"><summary>文件自带目录（{value.outline.length} 项，显示前 {Math.min(20, value.outline.length)} 项）</summary><ol className="corpus-samples">{value.outline.slice(0, 20).map((item, index) => <li key={index}>{item.title} · {item.page_number === null ? "没有页码定位" : `物理第 ${item.page_number} 页`}</li>)}</ol></details>}
    <details className="workflow-record"><summary>解析器与预检记录</summary><pre tabIndex={0}>{JSON.stringify({ parser: value.parser, limits: value.limits, ocr: value.ocr }, null, 2)}</pre></details>
  </div>;
}

/** Large single pages are read in explicit text slices, never silently shortened. */
function TextReader({ text }: { text: string }) {
  const [part, setPart] = useState(0);
  const size = 12000;
  const total = Math.max(1, Math.ceil(text.length / size));
  const current = Math.min(part, total - 1);
  let start = current * size;
  let end = Math.min(text.length, start + size);
  if (start > 0 && text.charCodeAt(start) >= 0xdc00 && text.charCodeAt(start) <= 0xdfff) start -= 1;
  if (end < text.length && text.charCodeAt(end) >= 0xdc00 && text.charCodeAt(end) <= 0xdfff) end -= 1;
  return <>
    <pre tabIndex={0}>{text.slice(start, end) || "此页没有已保存文字。"}</pre>
    {total > 1 && <div className="corpus-pagination" aria-label="原文分段阅读"><button className="button button-secondary" disabled={current === 0} onClick={() => setPart(current - 1)}>上一段原文</button><span>第 {current + 1} / {total} 段；完整文字分段显示</span><button className="button button-secondary" disabled={current + 1 >= total} onClick={() => setPart(current + 1)}>下一段原文</button></div>}
  </>;
}

function PagePreview({ page }: { page: CorpusPage }) {
  const [expanded, setExpanded] = useState(false);
  const [regionOffset, setRegionOffset] = useState(0);
  const limit = 20;
  return <article className="corpus-page">
    <div className="command-heading"><div><h3>物理第 {page.page_number} 页</h3><p>{corpusPageStatusLabel[page.status] ?? page.status} · {page.regions.length.toLocaleString()} 个区域 · {page.width_pt == null || page.height_pt == null ? "页面尺寸尚未解析" : `${page.width_pt} × ${page.height_pt} pt · 旋转 ${page.rotation ?? 0}°`}</p></div><button className="button button-secondary" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? "收起原文" : "查看本页原文"}</button></div>
    <Issues issues={page.issues} />
    {expanded && <div className="corpus-page-content"><h4>页面文字</h4><TextReader text={page.text} />
      {!page.page_revision && <p className="workflow-note">这是旧解析记录，尚未保存页面版本字段；原有文字和问题保留，不能据此确认新版本身份。</p>}
      <details className="workflow-record"><summary>页面版本与解析记录</summary><pre tabIndex={0}>{JSON.stringify({ source_id: page.source_id, page_number: page.page_number, page_revision: page.page_revision, parser: page.parser, geometry: page.geometry }, null, 2)}</pre></details>
      {page.regions.length > 0 && <><h4>来源区域与坐标</h4><p className="workflow-note">显示 {regionOffset + 1}–{Math.min(regionOffset + limit, page.regions.length)} / {page.regions.length.toLocaleString()} 个区域。</p>
        {page.regions.slice(regionOffset, regionOffset + limit).map((region) => <details className="workflow-record corpus-region" key={region.ordinal}><summary>区域 {region.ordinal + 1} · {region.kind} · 坐标 {region.bbox.join(", ")}</summary><TextReader text={region.text} /></details>)}
        {page.regions.length > limit && <div className="corpus-pagination" aria-label={`第 ${page.page_number} 页区域分页`}><button className="button button-secondary" disabled={regionOffset === 0} onClick={() => setRegionOffset(Math.max(0, regionOffset - limit))}>上一组区域</button><button className="button button-secondary" disabled={regionOffset + limit >= page.regions.length} onClick={() => setRegionOffset(regionOffset + limit)}>下一组区域</button></div>}
      </>}
    </div>}
  </article>;
}

export default function AppCorpus({ corpus: c }: { corpus: CorpusWorkbench }) {
  const [jumpPage, setJumpPage] = useState("");
  const selected = c.selected;
  const originalConfirmed = !!selected?.content_sha256 && selected.status !== "interrupted";
  const active = !!selected && c.runningIds.includes(selected.source_id);
  const pausing = !!selected && c.pauseIds.includes(selected.source_id);
  const total = selected?.page_count ?? c.pages?.total ?? 0;
  const jump = Number(jumpPage);
  const inputBusy = c.inputBusy !== null;
  return <div className="corpus-workbench">
    <section className="runtime-card" aria-labelledby="corpus-import-title" aria-busy={inputBusy}>
      <div className="runtime-title"><div><h2 id="corpus-import-title">导入整本 PDF</h2><p>先检查文件，再将原件保存到项目并逐页解析。已保存进度可在下次打开时继续。</p></div></div>
      <div className="project-path-row"><label htmlFor="corpus-path">PDF 文件</label><input id="corpus-path" value={c.path} disabled={inputBusy} spellCheck={false} placeholder="选择或填写 PDF 路径" onChange={(event) => c.choosePath(event.target.value)} /></div>
      <div className="runtime-actions"><button className="button button-secondary" disabled={inputBusy} onClick={() => void c.pickPdf()}>选择 PDF 文件</button><button className="button button-secondary" disabled={inputBusy || !c.path.trim()} onClick={() => void c.checkPdf()}>{c.inputBusy === "preflight" ? "正在检查 PDF…" : "检查 PDF"}</button><button className="button button-primary" disabled={inputBusy || c.preflight?.status !== "ready"} onClick={() => void c.importPdf()}>{c.inputBusy === "import" ? "正在保存原件…" : "导入并开始解析"}</button></div>
      {!c.preflight && <p className="workflow-note">检查后会显示文件大小、物理页数、密码状态、本机扫描识别能力和实际处理限制。原件保存期间可以切换页面，回到这里查看进度。</p>}
      {c.preflight && <Preflight value={c.preflight} />}
    </section>

    <section className="command-card" aria-labelledby="corpus-library-title">
      <div className="command-heading"><div><h2 id="corpus-library-title">项目资料库</h2><p>这里读取项目中已保存的资料。解析状态与知识整理状态分别记录。</p></div><button className="button button-secondary" disabled={c.listBusy} onClick={() => void c.refreshList()}>{c.listBusy ? "正在刷新…" : "刷新资料库"}</button></div>
      {c.listError && <p className="error-message" role="alert">资料库暂时无法刷新，以下保留上次读取结果。{c.listError}</p>}
      {c.sources.length === 0 && !c.listError && <p className="workflow-note">{c.listBusy ? "正在读取资料库…" : "这个项目还没有登记整本 PDF。选择文件并导入后会出现在这里。"}</p>}
      <div className="corpus-library" role="group" aria-label="已保存 PDF">{c.sources.map((source) => <button key={source.source_id} className={`corpus-source ${selected?.source_id === source.source_id ? "is-selected" : ""}`} aria-pressed={selected?.source_id === source.source_id} onClick={() => { c.selectSource(source.source_id); setJumpPage(""); }}><strong>{source.display_name}</strong><span>{byteSize(source.byte_length)} · {corpusStatusLabel[source.status]}</span><span>已记录 {source.processed_pages.toLocaleString()} / {source.page_count?.toLocaleString() ?? "未知"} 页{source.review_pages > 0 ? ` · 待复核 ${source.review_pages}` : ""}{source.failed_pages > 0 ? ` · 失败 ${source.failed_pages}` : ""}</span></button>)}</div>
    </section>

    {selected && <section className="command-card" aria-labelledby="corpus-progress-title">
      <div className="command-heading"><div><h2 id="corpus-progress-title">{selected.display_name}</h2><p>{corpusStatusLabel[selected.status]} · 知识整理尚未开始</p></div><span className="small-badge">物理页覆盖</span></div>
      <div className="corpus-progress"><progress aria-label="已保存页面进度" value={selected.page_count === null ? undefined : selected.processed_pages} max={Math.max(1, selected.page_count ?? 1)} /><span>已记录 {selected.processed_pages.toLocaleString()} / {selected.page_count?.toLocaleString() ?? "尚未确定"} 页</span></div>
      <dl className="runtime-details model-details"><div><dt>文字页</dt><dd>{selected.text_pages.toLocaleString()}</dd></div><div><dt>OCR 页</dt><dd>{selected.ocr_pages.toLocaleString()}</dd></div><div><dt>待识别页</dt><dd>{selected.needs_ocr_pages.toLocaleString()}</dd></div><div><dt>空白页</dt><dd>{selected.blank_pages.toLocaleString()}</dd></div><div><dt>待复核页</dt><dd>{selected.review_pages.toLocaleString()}</dd></div><div><dt>失败页</dt><dd>{selected.failed_pages.toLocaleString()}</dd></div></dl>
      {selected.error && <p className="error-message" role="alert">{selected.error.code}：{selected.error.message}</p>}
      {!originalConfirmed && <p className="workflow-note">{selected.status === "acquiring" ? "原件正在保存，暂不能解析。若获取中断，" : "原件获取尚未确认，暂不能解析。"}请使用上方“选择 PDF 文件”重新选取原件，检查后重新导入。</p>}
      {originalConfirmed && selected.status === "running" && !active && <p className="workflow-note">上次解析尚未返回结果。可以检查并继续，已经保存的页面会保留；正在运行的解析不会重复启动。</p>}
      {(selected.review_pages > 0 || selected.failed_pages > 0) && <p className="workflow-note">存在待复核或失败页面，请在下面查看页号和具体问题。原文可读不代表图像、公式或科学内容已核验。</p>}
      <div className="runtime-actions">
        {(active || selected.status === "running") && <button className="button button-secondary" disabled={pausing} onClick={() => void c.pauseSource(selected.source_id)}>{pausing ? "正在等待暂停…" : "暂停解析"}</button>}
        {originalConfirmed && !active && selected.status !== "acquiring" && selected.status !== "ready" && (selected.status !== "needs_review" || selected.processed_pages < (selected.page_count ?? 0) || selected.needs_ocr_pages > 0) && <button className="button button-primary" disabled={pausing} onClick={() => void c.processSource(selected.source_id)}>{selected.status === "running" ? "检查并继续" : selected.status === "queued" ? "开始解析" : selected.needs_ocr_pages > 0 && selected.processed_pages === selected.page_count ? "继续扫描识别" : "继续未处理页"}</button>}
        {originalConfirmed && !active && selected.failed_pages > 0 && <button className="button button-secondary" disabled={pausing} onClick={() => void c.processSource(selected.source_id, true)}>重试失败页（{selected.failed_pages} 页）</button>}
      </div>
      <p className="workflow-note">已记录页数包含空白页、待复核页和失败页。暂停后可继续；整书知识抽取与笔记生成尚未接入此资料库。</p>
      {originalConfirmed && selected.needs_ocr_pages > 0 && <p className="workflow-note">仍有 {selected.needs_ocr_pages.toLocaleString()} 页等待扫描识别。继续时会使用已配置并通过运行检查的本机 OCR；尚未配置时保留待识别页。</p>}
      {originalConfirmed && selected.failed_pages > 0 && <p className="workflow-note">重试只重新处理失败页，已经保存的成功页和历史页版本会保留。</p>}
      <details className="workflow-record"><summary>原件身份与保存记录</summary><dl className="runtime-details"><div><dt>原件 SHA256</dt><dd>{selected.content_sha256 ?? "原件尚未保存完成"}</dd></div><div><dt>登记时间</dt><dd>{selected.created_at}</dd></div><div><dt>资料 ID</dt><dd>{selected.source_id}</dd></div></dl></details>

      <div className="corpus-pages-heading"><h3>已保存页面与原文</h3><button className="button button-secondary" disabled={c.pagesBusy} onClick={() => void c.refreshPages()}>刷新页面</button></div>
      <div className="corpus-pagination" aria-label="PDF 页面分页"><button className="button button-secondary" disabled={c.pagesBusy || c.offset === 0} onClick={() => c.setOffset(Math.max(0, c.offset - c.pageSize))}>上一组页面</button><span>{total > 0 ? `物理第 ${c.offset + 1}–${Math.min(c.offset + c.pageSize, total)} 页，共 ${total.toLocaleString()} 页` : "页数尚未确定"}</span><button className="button button-secondary" disabled={c.pagesBusy || c.offset + c.pageSize >= total} onClick={() => c.setOffset(c.offset + c.pageSize)}>下一组页面</button></div>
      <form className="corpus-jump" onSubmit={(event) => { event.preventDefault(); if (Number.isInteger(jump) && jump >= 1 && jump <= total) c.setOffset(Math.floor((jump - 1) / c.pageSize) * c.pageSize); }}><label htmlFor="corpus-page-jump">跳到物理页</label><input id="corpus-page-jump" type="number" min={1} max={total || undefined} value={jumpPage} onChange={(event) => setJumpPage(event.target.value)} /><button className="button button-secondary" disabled={c.pagesBusy || !Number.isInteger(jump) || jump < 1 || jump > total}>前往</button></form>
      {c.pagesError && <p className="error-message" role="alert">页面暂时无法读取。{c.pagesError}</p>}
      {c.pagesBusy && <p className="settings-status" role="status">正在读取这一组页面…</p>}
      {c.pages?.pages.length === 0 && !c.pagesBusy && !c.pagesError && <p className="workflow-note">这一组页面尚未保存，解析后可以查看原文和问题。</p>}
      <div aria-busy={c.pagesBusy}>{c.pages?.pages.map((page) => <PagePreview key={`${selected.source_id}-${page.page_number}-${page.page_revision ?? "pending"}`} page={page} />)}</div>
    </section>}
    {c.notice && <p className="settings-status" role="status">{c.notice}</p>}
    {c.error && <p className="error-message" role="alert">{c.error}</p>}
  </div>;
}
