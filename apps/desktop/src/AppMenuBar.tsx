import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

export interface MenuAction {
  id: string;
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  checked?: boolean;
}

export interface AppMenu {
  label: "File" | "Edit" | "View" | "Help";
  actions: MenuAction[];
}

interface AppMenuBarProps {
  canGoBack: boolean;
  canGoForward: boolean;
  sidebarVisible: boolean;
  onBack: () => void;
  onForward: () => void;
  onToggleSidebar: () => void;
  menus: AppMenu[];
}

function NavigationIcon({ name }: { name: "back" | "forward" | "sidebar" }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      {name === "sidebar"
        ? <><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><path d="M9 4.5v15" /></>
        : <path d={name === "back" ? "M19 12H5m6-6-6 6 6 6" : "M5 12h14m-6-6 6 6-6 6"} />}
    </svg>
  );
}

export default function AppMenuBar(props: AppMenuBarProps) {
  const [openMenu, setOpenMenu] = useState<number | null>(null);
  const [activeTrigger, setActiveTrigger] = useState(0);
  const [position, setPosition] = useState({ left: 8, top: 36 });
  const menubarRef = useRef<HTMLDivElement>(null);
  const triggerRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const initialItem = useRef<"first" | "last">("first");
  const { menus } = props;

  function closeMenu(restoreFocus = false) {
    if (restoreFocus && openMenu !== null) triggerRefs.current[openMenu]?.focus();
    setOpenMenu(null);
  }

  function showMenu(index: number, edge: "first" | "last" = "first") {
    initialItem.current = edge;
    setActiveTrigger(index);
    setOpenMenu(index);
    if (openMenu === index) {
      const enabled = itemRefs.current.filter((item) => item && !item.disabled);
      (edge === "last" ? enabled.at(-1) : enabled[0])?.focus();
    }
  }

  useLayoutEffect(() => {
    if (openMenu === null) return;
    function updatePosition() {
      const bounds = triggerRefs.current[openMenu!]?.getBoundingClientRect();
      if (!bounds) return;
      const width = Math.min(224, window.innerWidth - 16);
      setPosition({ left: Math.max(8, Math.min(bounds.left, window.innerWidth - width - 8)), top: bounds.bottom + 3 });
    }
    updatePosition();
    const enabled = itemRefs.current.filter((item) => item && !item.disabled);
    (initialItem.current === "last" ? enabled.at(-1) : enabled[0])?.focus();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [openMenu]);

  useEffect(() => {
    if (openMenu === null) return;
    function outsidePointer(event: PointerEvent) {
      if (event.target instanceof Node && !menubarRef.current?.contains(event.target)) {
        setOpenMenu(null);
      }
    }
    function outsideFocus(event: FocusEvent) {
      if (event.target instanceof Node && !menubarRef.current?.contains(event.target)) setOpenMenu(null);
    }
    document.addEventListener("pointerdown", outsidePointer);
    document.addEventListener("focusin", outsideFocus);
    return () => {
      document.removeEventListener("pointerdown", outsidePointer);
      document.removeEventListener("focusin", outsideFocus);
    };
  }, [openMenu]);

  function triggerKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    if (event.key === "ArrowLeft" || event.key === "ArrowRight" || event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const next = event.key === "Home" ? 0 : event.key === "End" ? menus.length - 1
        : (index + (event.key === "ArrowRight" ? 1 : -1) + menus.length) % menus.length;
      setActiveTrigger(next);
      triggerRefs.current[next]?.focus();
      if (openMenu !== null) showMenu(next);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      showMenu(index, event.key === "ArrowUp" ? "last" : "first");
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
    }
  }

  function itemKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    if (openMenu === null) return;
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const enabledIndexes = menus[openMenu].actions.flatMap((action, i) => action.disabled ? [] : [i]);
      const current = enabledIndexes.indexOf(index);
      const next = event.key === "Home" ? enabledIndexes[0] : event.key === "End" ? enabledIndexes.at(-1)
        : enabledIndexes[(current + (event.key === "ArrowDown" ? 1 : -1) + enabledIndexes.length) % enabledIndexes.length];
      if (next !== undefined) itemRefs.current[next]?.focus();
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      showMenu((openMenu + (event.key === "ArrowRight" ? 1 : -1) + menus.length) % menus.length);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeMenu(true);
    } else if (event.key === "Tab") {
      // Restore the trigger so the browser's normal Tab order can leave the menu.
      closeMenu(true);
    }
  }

  return (
    <header className="app-menu-bar" aria-label="应用菜单栏">
      <div className="app-navigation" aria-label="页面导航">
        <button className="app-navigation-button" aria-label="后退" title="后退" disabled={!props.canGoBack} onClick={props.onBack}><NavigationIcon name="back" /></button>
        <button className="app-navigation-button" aria-label="前进" title="前进" disabled={!props.canGoForward} onClick={props.onForward}><NavigationIcon name="forward" /></button>
        <button className="app-navigation-button" aria-label="切换侧栏" title="切换侧栏" aria-controls="app-sidebar" aria-expanded={props.sidebarVisible} onClick={props.onToggleSidebar}><NavigationIcon name="sidebar" /></button>
      </div>
      <div className="app-menubar" role="menubar" aria-label="应用菜单" ref={menubarRef}>
        {menus.map((menu, index) => (
          <button key={menu.label} id={`app-menu-${menu.label.toLowerCase()}`} className="app-menu-trigger" role="menuitem" aria-haspopup="menu" aria-expanded={openMenu === index} aria-controls={openMenu === index ? "app-menu-dropdown" : undefined} tabIndex={activeTrigger === index ? 0 : -1} ref={(element) => { triggerRefs.current[index] = element; }} onFocus={() => setActiveTrigger(index)} onKeyDown={(event) => triggerKeyDown(event, index)} onClick={() => openMenu === index ? closeMenu(true) : showMenu(index)}>{menu.label}</button>
        ))}
        {openMenu !== null && (
          <div key={openMenu} id="app-menu-dropdown" className="app-menu-dropdown" role="menu" aria-labelledby={`app-menu-${menus[openMenu].label.toLowerCase()}`} style={{ left: position.left, top: position.top, maxHeight: `calc(100vh - ${position.top + 8}px)` }}>
            {menus[openMenu].actions.map((action, index) => (
              <button key={action.id} className="app-menu-action" role={action.checked === undefined ? "menuitem" : "menuitemcheckbox"} aria-checked={action.checked} disabled={action.disabled} tabIndex={-1} ref={(element) => { itemRefs.current[index] = element; }} onKeyDown={(event) => itemKeyDown(event, index)} onClick={() => { closeMenu(true); action.onSelect(); }}>
                <span className="app-menu-check" aria-hidden="true">{action.checked ? "✓" : ""}</span><span>{action.label}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </header>
  );
}
