import { mockState } from "./mock";
import type {
  AnalysisProgress,
  BatchDownloadResult,
  BatchItemSelection,
  BootstrapState,
  BrowserSource,
  CookieImportRequest,
  CookieImportResult,
  DownloadHistoryEntry,
  DownloadRequest,
  DownloadTask,
  PlatformId,
  ProfileBatch,
  SaveSettingsPayload,
  SettingsProfile,
  TaskEvent,
  UpdateCheckResult,
  VideoAsset
} from "./types";

declare global {
  interface Window { __TAURI_INTERNALS__?: unknown; }
}

export function hasTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

/** Windows 桌面端使用无边框窗口 + 自绘标题栏；macOS 保留原生窗框，浏览器预览不启用 */
export function isFramelessWindows() {
  return hasTauriRuntime() && typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
}

/** macOS 桌面端：原生红绿灯 + 隐藏标题栏（hiddenTitle + Overlay），需要顶栏拖拽区与毛玻璃适配 */
export function isMacOS() {
  return hasTauriRuntime() && typeof navigator !== "undefined" && /Mac OS X|macOS/.test(navigator.userAgent);
}

function desktopRuntimeRequired(): never {
  throw new Error("浏览器地址仅用于界面预览，无法调用本机解析器。请运行桌面版 StreamVerse 解析真实链接。");
}

async function invoke<T>(command: string, payload?: unknown): Promise<T> {
  if (!hasTauriRuntime()) throw new Error(`Tauri runtime unavailable: ${command}`);
  const api = await import("@tauri-apps/api/core");
  return api.invoke<T>(command, payload as Record<string, unknown> | undefined);
}

export async function getBootstrapState(): Promise<BootstrapState> {
  return hasTauriRuntime() ? invoke("get_bootstrap_state") : structuredClone(mockState);
}

export async function analyzeInput(rawInput: string, sessionId = crypto.randomUUID()): Promise<VideoAsset> {
  return hasTauriRuntime()
    ? invoke("analyze_input", { rawInput, sessionId })
    : desktopRuntimeRequired();
}

export async function analyzeBatchItem(rawInput: string): Promise<VideoAsset> {
  return hasTauriRuntime()
    ? invoke("analyze_input", { rawInput, sessionId: null })
    : desktopRuntimeRequired();
}

export async function analyzeProfileInput(rawInput: string, sessionId = crypto.randomUUID()): Promise<ProfileBatch> {
  if (hasTauriRuntime()) {
    return invoke("analyze_profile_input", { rawInput, sessionId });
  }
  return desktopRuntimeRequired();
}

export async function getAnalysisProgress(sessionId: string): Promise<AnalysisProgress | null> {
  return hasTauriRuntime() ? invoke("get_analysis_progress", { sessionId }) : null;
}

export async function clearAnalysisProgress(sessionId: string): Promise<void> {
  if (hasTauriRuntime()) await invoke("clear_analysis_progress", { sessionId });
}

export async function fetchThumbnail(url: string): Promise<string> {
  return hasTauriRuntime() ? invoke("fetch_thumbnail", { url }) : url;
}

export async function createDownloadTask(request: DownloadRequest): Promise<DownloadTask> {
  if (hasTauriRuntime()) return invoke("create_download_task", { request });
  return desktopRuntimeRequired();
}

export async function createProfileDownloadTasks(payload: {
  profileTitle: string;
  sourceUrl: string;
  items: BatchItemSelection[];
  saveDirectoryOverride?: string | null;
  downloadOptions: DownloadRequest["downloadOptions"];
}): Promise<BatchDownloadResult> {
  if (hasTauriRuntime()) return invoke("create_profile_download_tasks", { request: payload });
  return desktopRuntimeRequired();
}

export async function subscribeTaskEvents(onEvent: (event: TaskEvent) => void) {
  if (!hasTauriRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<TaskEvent>("task-event", ({ payload }) => onEvent(payload));
}

export async function controlTask(taskId: string, action: "pause" | "resume" | "cancel" | "retry") {
  return invoke<DownloadTask>(`${action}_download_task`, { taskId });
}

export async function clearFinishedTasks(): Promise<DownloadTask[]> {
  return hasTauriRuntime() ? invoke("clear_finished_tasks") : [];
}

export async function removeDownloadTask(taskId: string): Promise<void> {
  if (hasTauriRuntime()) await invoke("remove_download_task", { taskId });
}

export async function saveSettings(payload: SaveSettingsPayload): Promise<SettingsProfile> {
  if (hasTauriRuntime()) return invoke("save_settings", { request: payload });
  return { ...mockState, ...payload };
}

export async function pickSaveDirectory(currentDirectory: string): Promise<string | null> {
  return hasTauriRuntime()
    ? invoke("pick_save_directory", { currentDirectory })
    : currentDirectory;
}

export async function listBrowserSources(): Promise<BrowserSource[]> {
  if (hasTauriRuntime()) return invoke("list_browser_sources");
  return [
    {
      id: "edge",
      label: "Microsoft Edge",
      isDefault: true,
      profiles: [{ id: "edge-default", label: "Default", isDefault: true }]
    },
    {
      id: "chrome",
      label: "Google Chrome",
      isDefault: false,
      profiles: [{ id: "chrome-default", label: "Personal", isDefault: true }]
    }
  ];
}

export async function importBrowserCookies(request: CookieImportRequest): Promise<CookieImportResult> {
  if (hasTauriRuntime()) return invoke("import_browser_cookies", { request });
  return {
    platform: request.platform,
    browserId: request.browserId,
    profileId: request.profileId,
    status: "active",
    importedCount: 12,
    requiresElevation: false,
    message: "Browser session imported"
  };
}

export async function pickCookieFile(): Promise<string | null> {
  return hasTauriRuntime() ? invoke("pick_cookie_file", { currentFile: null }) : null;
}

export async function saveManualCookies(
  platform: PlatformId,
  source: { cookieText?: string; cookieFile?: string }
) {
  return hasTauriRuntime()
    ? invoke<CookieImportResult>("save_manual_cookies", {
        platform,
        cookieText: source.cookieText ?? null,
        cookieFile: source.cookieFile ?? null
      })
    : ({
        platform,
        browserId: "manual",
        status: "active",
        importedCount: 3,
        requiresElevation: false,
        message: "Manual session saved"
      } satisfies CookieImportResult);
}

export async function clearPlatformAuth(platform: PlatformId): Promise<void> {
  if (hasTauriRuntime()) await invoke("clear_platform_auth", { platform });
}

export async function openInFileManager(path: string, revealParent = false): Promise<void> {
  if (hasTauriRuntime()) await invoke("open_in_file_manager", { path, revealParent });
}

export async function listDownloadHistory(limit = 100, platform?: PlatformId): Promise<DownloadHistoryEntry[]> {
  return hasTauriRuntime() ? invoke("list_download_history", { limit, platform }) : [];
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  return hasTauriRuntime() ? invoke("check_for_update") : desktopRuntimeRequired();
}

export async function openExternalUrl(url: string): Promise<void> {
  return hasTauriRuntime() ? invoke("open_external_url", { url }) : desktopRuntimeRequired();
}
