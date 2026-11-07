import { useEffect, useRef, useState } from "react";
import AppIcon from "./AppIcon";
import type { Page, Theme } from "./appTypes";

const pageLabels: Record<Page, string> = {
  workspace: "工作台", project: "项目", runtime: "运行检查", appearance: "外观设置", window: "窗口设置", about: "关于 Noetome",
};

interface AppSidebarProps {
  page: Page;
  visible: boolean;
  recentPages: Page[];
  theme: Theme;
  onNavigate: (page: Page) => void;
  onToggleTheme: () => void;
}

export default function AppSidebar({ page, visible, recentPages, theme, onNavigate, onToggleTheme }: AppSidebarProps) {
  const [searchVisible, setSearchVisible] = useState(false);
  const [query, setQuery] = useState("");
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const searchRef = useRef<HTMLInputElement>(null);
  const searchButtonRef = useRef<HTMLButtonElement>(null);
  const inSettings = page === "appearance" || page === "window" || page === "about";
  const search = query.trim().toLocaleLowerCase();
  const matches = (target: Page) => pageLabels[target].toLocaleLowerCase().includes(search);
  const results = (Object.keys(pageLabels) as Page[]).filter(matches);
  const tools = (["runtime", "about"] as const).filter(matches);
  const recent = recentPages.filter(matches);

  useEffect(() => {
    if (searchVisible && visible) searchRef.current?.focus();
  }, [searchVisible, visible]);

  function closeSearch() {
    setSearchVisible(false);
    setQuery("");
    searchButtonRef.current?.focus();
  }

  function pageButton(target: Page, icon?: "pulse" | "info") {
    return (
      <button key={target} className={`nav-item ${page === target ? "is-active" : ""}`} aria-current={page === target ? "page" : undefined} onClick={() => onNavigate(target)}>
        {icon && <AppIcon name={icon} />}<span>{pageLabels[target]}</span>
      </button>
    );
  }

  return (
    <>
      <nav className="activity-rail" aria-label="快捷导航">
        <div className="rail-top">
          <button className={`rail-button ${page === "workspace" ? "is-active" : ""}`} aria-label="工作台" title="工作台" aria-current={page === "workspace" ? "page" : undefined} onClick={() => onNavigate("workspace")}><AppIcon name="home" filled={page === "workspace"} /></button>
          <button className={`rail-button ${page === "project" ? "is-active" : ""}`} aria-label="项目" title="项目" aria-current={page === "project" ? "page" : undefined} onClick={() => onNavigate("project")}><AppIcon name="folder" filled={page === "project"} /></button>
          <button className={`rail-button ${page === "runtime" ? "is-active" : ""}`} aria-label="运行检查" title="运行检查" aria-current={page === "runtime" ? "page" : undefined} onClick={() => onNavigate("runtime")}><AppIcon name="pulse" /></button>
          <button className="rail-button" aria-label="关于 Noetome" title="关于 Noetome" onClick={() => onNavigate("about")}><AppIcon name="more" /></button>
        </div>
        <div className="rail-bottom">
          <button className="rail-button theme-toggle" aria-label={theme === "light" ? "切换到暗色" : "切换到亮色"} title={theme === "light" ? "切换到暗色" : "切换到亮色"} onClick={onToggleTheme}><AppIcon name={theme === "light" ? "moon" : "sun"} /></button>
          <button className={`rail-button ${inSettings ? "is-active" : ""}`} aria-label="设置" title="设置" aria-current={inSettings ? "page" : undefined} onClick={() => onNavigate("appearance")}><AppIcon name="settings" /></button>
        </div>
      </nav>

      <aside id="app-sidebar" className="sidebar" aria-label="应用导航" hidden={!visible}>
        <header className="sidebar-heading">
          <a className="brand" href="#workspace" aria-label="Noetome 工作台" onClick={(event) => { event.preventDefault(); onNavigate("workspace"); }}>Noetome</a>
          <button className={`sidebar-tool ${searchVisible ? "is-active" : ""}`} aria-label="搜索导航" title="搜索导航" aria-expanded={searchVisible} aria-controls="navigation-search" ref={searchButtonRef} onClick={() => searchVisible ? closeSearch() : setSearchVisible(true)}><AppIcon name="search" /></button>
        </header>

        {searchVisible && <div className="sidebar-search"><input id="navigation-search" type="search" aria-label="搜索导航页面" placeholder="搜索导航…" ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") closeSearch(); }} /></div>}
        {!search && <button className="sidebar-primary" onClick={() => onNavigate("workspace")}><AppIcon name="compose" /><span>工作台</span></button>}

        <div className="sidebar-scroll">
          {!search && <section className="sidebar-section" aria-labelledby="projects-heading">
            <h2 id="projects-heading" className="nav-section-label">项目</h2>
            <button className="project-toggle" aria-expanded={projectsExpanded} aria-controls="sidebar-projects" onClick={() => setProjectsExpanded((expanded) => !expanded)}><AppIcon name="folder" /><span>学习项目</span><span className={`project-caret ${projectsExpanded ? "is-expanded" : ""}`}><AppIcon name="caret" /></span></button>
            <div id="sidebar-projects" className="sidebar-empty" hidden={!projectsExpanded}><p>当前会话尚未载入项目</p><button className="text-button" onClick={() => onNavigate("project")}>新建或打开项目<AppIcon name="arrow" /></button></div>
          </section>}

          {!search && tools.length > 0 && <section className="sidebar-section" aria-labelledby="tools-heading"><h2 id="tools-heading" className="nav-section-label">工具</h2><nav aria-label="工具导航">{tools.map((target) => pageButton(target, target === "runtime" ? "pulse" : "info"))}</nav></section>}
          {!search && recent.length > 0 && <section className="sidebar-section recent-section" aria-labelledby="recent-heading"><h2 id="recent-heading" className="nav-section-label">最近访问</h2><nav aria-label="最近访问">{recent.map((target) => pageButton(target))}</nav></section>}
          {search && <section className="sidebar-section" aria-labelledby="search-results-heading"><h2 id="search-results-heading" className="nav-section-label">搜索结果</h2>{results.length > 0 ? <nav aria-label="搜索结果">{results.map((target) => pageButton(target))}</nav> : <p className="navigation-empty" role="status">没有匹配的页面</p>}</section>}
        </div>
      </aside>
    </>
  );
}
