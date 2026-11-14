import Icon from "./AppIcon";
import AppCorpus from "./AppCorpus";
import AppLegacySources from "./AppLegacySources";
import type { Workbench } from "./useWorkbench";
import type { CorpusWorkbench } from "./useCorpus";

export default function AppSources({ workbench, corpus, view, onView, onProject, onModel }: {
  workbench: Workbench; corpus: CorpusWorkbench; view: "pdf" | "legacy";
  onView: (view: "pdf" | "legacy") => void; onProject: () => void; onModel: () => void;
}) {
  return <>
    <div className="page-heading"><div><p className="eyebrow">YOUR SOURCES</p><h1>从自己的资料开始。</h1><p className="page-description">把整本资料保存到项目，查看逐页原文与处理问题。</p></div></div>
    {!workbench.project ? <section className="runtime-card"><h2>{workbench.projectBusy ? "正在恢复项目…" : "先选择一个项目"}</h2><p className="settings-description">资料与处理记录会保存在所选项目中。</p>{workbench.projectForm.error && <p className="error-message" role="alert">{workbench.projectForm.error}</p>}<button className="button button-primary" disabled={workbench.projectBusy} onClick={onProject}>建立或打开项目<Icon name="arrow" /></button></section> : <>
      <p className="workflow-project">当前项目：<strong>{workbench.project.path}</strong></p>
      <div className="workflow-tabs corpus-tabs" role="group" aria-label="资料类型"><button className={`button button-secondary ${view === "pdf" ? "is-selected" : ""}`} aria-pressed={view === "pdf"} onClick={() => onView("pdf")}>整本 PDF</button><button className={`button button-secondary ${view === "legacy" ? "is-selected" : ""}`} aria-pressed={view === "legacy"} onClick={() => onView("legacy")}>文本与旧资料</button></div>
      {view === "pdf" ? <AppCorpus corpus={corpus} /> : <AppLegacySources workbench={workbench} onProject={onProject} onModel={onModel} />}
    </>}
  </>;
}
