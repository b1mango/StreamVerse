import { isMacOS } from "./backend";

// macOS 隐藏标题栏（红绿灯悬浮）下，顶栏与侧栏空白区充当窗口拖拽区，双击缩放
export function createMacShellDrag() {
  const macDesktop = isMacOS();
  let appWindow: import("@tauri-apps/api/window").Window | null = null;
  if (macDesktop) {
    void import("@tauri-apps/api/window").then((api) => { appWindow = api.getCurrentWindow(); });
  }

  function macDragHit(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    if (!target || target.closest("button, a, input, textarea, select, [contenteditable]")) return false;
    return Boolean(target.closest(".stage-header, .task-header, .nav-rail, .mac-titlebar"));
  }

  function handleMouseDown(event: MouseEvent) {
    if (!macDesktop || event.button !== 0 || !macDragHit(event)) return;
    void appWindow?.startDragging();
  }

  function handleDblClick(event: MouseEvent) {
    if (!macDesktop || !macDragHit(event)) return;
    void appWindow?.toggleMaximize();
  }

  return { macDesktop, handleMouseDown, handleDblClick };
}
