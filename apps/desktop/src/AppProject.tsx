import Icon from "./AppIcon";
import type { Workbench } from "./useWorkbench";

export default function AppProject({ workbench, onSources }: { workbench: Workbench; onSources: () => void }) {
  const { project, projectForm: form, setProjectForm, projectBusy: busy } = workbench;
  const creating = form.mode === "create";
  const ready = creating ? form.parent.trim() && form.name.trim() : form.path.trim();
  return <>
    <div className="page-heading"><div><p className="eyebrow">PROJECT</p><h1>建立或打开一个项目。</h1><p className="page-description">选择项目后，导入自己的资料，整理为可追溯的待核验笔记。</p></div></div>
    <section className="runtime-card" aria-labelledby="project-title" aria-busy={busy}>
      <div className="runtime-card-heading"><div className="runtime-title"><div className="card-icon"><Icon name="folder" /></div><div><h2 id="project-title">项目目录</h2><p>新建项目放在新的子目录中；打开已有项目会先检查本地记录。</p></div></div></div>
      <div className="workflow-tabs" role="group" aria-label="项目操作">
        <button className={`button button-secondary ${creating ? "is-selected" : ""}`} disabled={busy} aria-pressed={creating} onClick={() => setProjectForm((previous) => ({ ...previous, mode: "create", error: "", notice: "" }))}>新建</button>
        <button className={`button button-secondary ${!creating ? "is-selected" : ""}`} disabled={busy} aria-pressed={!creating} onClick={() => setProjectForm((previous) => ({ ...previous, mode: "open", error: "", notice: "" }))}>打开已有项目</button>
      </div>
      <div className="project-path-row"><label htmlFor="project-path">{creating ? "项目父目录" : "项目目录"}</label><input id="project-path" type="text" spellCheck={false} disabled={busy} value={creating ? form.parent : form.path} placeholder="例如 D:\Noetome\学习资料" onChange={(event) => setProjectForm((previous) => ({ ...previous, [creating ? "parent" : "path"]: event.target.value }))} /></div>
      {creating && <div className="project-path-row"><label htmlFor="project-name">新项目名称</label><input id="project-name" value={form.name} disabled={busy} onChange={(event) => setProjectForm((previous) => ({ ...previous, name: event.target.value }))} /></div>}
      <div className="runtime-actions"><button className="button button-secondary" disabled={busy} onClick={() => void workbench.pickProjectDirectory()}>{creating ? "选择父目录" : "选择项目目录"}</button><button className="button button-primary" disabled={busy || !ready} onClick={() => void workbench.openProject()}>{busy ? "处理中…" : creating ? "新建项目" : "打开项目"}</button></div>
      {form.notice && <p className="settings-status" role="status">{form.notice}</p>}
      {form.error && <p className="error-message" role="alert">{form.error}</p>}
    </section>
    {project && <section className="command-card" aria-labelledby="project-fields-title">
      <div className="command-heading"><div><h2 id="project-fields-title" tabIndex={-1}>当前项目</h2><p>以下内容来自本地项目记录。</p></div><button className="button button-primary" onClick={onSources}>前往资料<Icon name="arrow" /></button></div>
      <dl className="runtime-details project-details"><div><dt>项目目录</dt><dd>{project.path}</dd></div><div><dt>项目 ID</dt><dd>{project.project_id}</dd></div><div><dt>结构版本</dt><dd>{project.schema_version}</dd></div><div><dt>创建时间</dt><dd>{project.created_at}</dd></div><div><dt>状态</dt><dd>{project.state}</dd></div><div><dt>数据库文件</dt><dd>{project.database}</dd></div></dl>
    </section>}
  </>;
}
