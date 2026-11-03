import { useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";

export default function WindowControls({ onError }: { onError: (message: string) => void }) {
  const [nativeWindow] = useState(() => isTauri() ? getCurrentWindow() : null);
  const [maximized, setMaximized] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const operationPending = useRef(false);
  const querySequence = useRef(0);

  function reportError(operation: string, error: unknown) {
    const detail = typeof error === "string" ? error : error instanceof Error ? error.message : "窗口没有返回可用结果。";
    onError(`${operation}失败：${detail} 请重试；仍失败时请重新打开 Noetome。`);
  }

  useEffect(() => {
    if (!nativeWindow) return;
    let alive = true;
    let unlisten: UnlistenFn | undefined;
    async function refresh() {
      const sequence = ++querySequence.current;
      try {
        const value = await nativeWindow!.isMaximized();
        if (alive && sequence === querySequence.current) setMaximized(value);
      } catch (error) {
        if (alive && sequence === querySequence.current) {
          setMaximized(null);
          reportError("读取窗口状态", error);
        }
      }
    }
    void (async () => {
      try {
        unlisten = await nativeWindow.onResized(() => { void refresh(); });
        if (!alive) { await unlisten(); return; }
        await refresh();
      } catch (error) {
        if (alive) reportError("监听窗口状态", error);
      }
    })();
    return () => {
      alive = false;
      if (unlisten) void Promise.resolve(unlisten()).catch(error => reportError("停止监听窗口状态", error));
    };
  }, [nativeWindow, onError]);

  async function operate(operation: "minimize" | "maximize" | "close") {
    if (!nativeWindow || operationPending.current || (operation === "maximize" && maximized === null)) return;
    operationPending.current = true;
    setBusy(true);
    onError("");
    try {
      if (operation === "minimize") await nativeWindow.minimize();
      else if (operation === "close") await nativeWindow.close();
      else {
        await nativeWindow.toggleMaximize();
        const sequence = ++querySequence.current;
        const value = await nativeWindow.isMaximized();
        if (sequence === querySequence.current) setMaximized(value);
      }
    } catch (error) {
      if (operation === "maximize") setMaximized(null);
      reportError(operation === "minimize" ? "最小化" : operation === "close" ? "关闭窗口" : "最大化/还原", error);
    } finally {
      operationPending.current = false;
      setBusy(false);
    }
  }

  function dragMouseDown(event: MouseEvent<HTMLDivElement>) {
    if (!nativeWindow || event.button !== 0 || event.target !== event.currentTarget) return;
    event.preventDefault();
    // No data-tauri-drag-region: Tauri's injected handler must not also toggle.
    if (event.detail === 2) void operate("maximize");
    else if (event.detail === 1) {
      onError("");
      void nativeWindow.startDragging().catch(error => reportError("拖动窗口", error));
    }
  }

  const maximizeLabel = maximized ? "还原" : "最大化";
  return <>
    <div className="app-window-drag-area" aria-hidden="true" onMouseDown={dragMouseDown} />
    <div className="app-window-controls" role="group" aria-label="窗口操作" title={nativeWindow ? undefined : "请在 Noetome 桌面窗口中使用"}>
      <button className="app-window-button" aria-label="最小化" title="最小化" disabled={!nativeWindow || busy} onClick={() => { void operate("minimize"); }}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="M3 8.5h10" /></svg>
      </button>
      <button className="app-window-button" aria-label={maximizeLabel} title={maximizeLabel} disabled={!nativeWindow || busy || maximized === null} onClick={() => { void operate("maximize"); }}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none" stroke="currentColor">{maximized ? <><path d="M5.5 4.5v-2h8v8h-2" /><rect x="2.5" y="5.5" width="8" height="8" /></> : <rect x="3.5" y="3.5" width="9" height="9" />}</svg>
      </button>
      <button className="app-window-button app-window-close" aria-label="关闭窗口" title="关闭窗口" disabled={!nativeWindow || busy} onClick={() => { void operate("close"); }}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="m3.5 3.5 9 9m0-9-9 9" /></svg>
      </button>
    </div>
  </>;
}
