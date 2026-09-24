import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Light integrated title bar (replaces OS decorations). */
export function TitleBar({ className = "" }: { className?: string }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    void win.isMaximized().then(setMaximized).catch(() => undefined);
    let unlisten: (() => void) | undefined;
    void win
      .onResized(() => {
        void win.isMaximized().then(setMaximized).catch(() => undefined);
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  const minimize = useCallback(() => {
    void getCurrentWindow().minimize();
  }, []);

  const toggleMaximize = useCallback(() => {
    void getCurrentWindow().toggleMaximize();
  }, []);

  const close = useCallback(() => {
    // Backend intercepts CloseRequested while mounted → hides to tray, keeps V: alive.
    void getCurrentWindow().close();
  }, []);

  return (
    <div
      className={`flex h-10 shrink-0 select-none items-stretch border-b border-[#e8e8e8] bg-[#f4f4f4] ${className}`}
      data-tauri-drag-region
    >
      <div className="flex-1" data-tauri-drag-region />
      <div className="flex items-stretch">
        <div className="mx-1 my-2 w-px bg-[#ddd]" />
        <button
          type="button"
          aria-label="Minimize"
          onClick={minimize}
          className="flex w-11 items-center justify-center text-[#444] transition hover:bg-black/5"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
            <path d="M2 6h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
        </button>
        <button
          type="button"
          aria-label={maximized ? "Restore" : "Maximize"}
          onClick={toggleMaximize}
          className="flex w-11 items-center justify-center text-[#444] transition hover:bg-black/5"
        >
          {maximized ? (
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden fill="none">
              <path d="M3.5 4.5h5v5h-5v-5Z" stroke="currentColor" strokeWidth="1.3" />
              <path d="M4.5 3.5h5v5" stroke="currentColor" strokeWidth="1.3" />
            </svg>
          ) : (
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden fill="none">
              <rect x="2.5" y="2.5" width="7" height="7" stroke="currentColor" strokeWidth="1.3" />
            </svg>
          )}
        </button>
        <button
          type="button"
          aria-label="Close"
          title="Close (keeps drive mounted)"
          onClick={close}
          className="flex w-11 items-center justify-center text-[#444] transition hover:bg-[#e81123] hover:text-white"
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden>
            <path d="M3 3l6 6M9 3 3 9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
        </button>
      </div>
    </div>
  );
}
