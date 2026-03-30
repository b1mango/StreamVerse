import { mockState } from "./mock";
import type {
  AnalyzeInputPayload,
  AnalyzeProfilePayload,
  BatchDownloadResult,
  BootstrapState,
  CreateProfileDownloadTasksPayload,
  CreateTaskPayload,
  DownloadTask,
  ProfileBatch,
  SaveSettingsPayload,
  SettingsProfile,
  VideoAsset
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

function hasTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

async function maybeInvoke<T>(command: string, payload?: unknown): Promise<T> {
  if (!hasTauriRuntime()) {
    throw new Error(`Tauri runtime unavailable for command: ${command}`);
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, payload as Record<string, unknown> | undefined);
}

export async function getBootstrapState(): Promise<BootstrapState> {
  if (!hasTauriRuntime()) {
    return mockState;
  }

  return maybeInvoke<BootstrapState>("get_bootstrap_state");
}

export async function analyzeInput(
  payload: AnalyzeInputPayload
): Promise<VideoAsset> {
  if (!hasTauriRuntime()) {
    return mockState.preview;
  }

  return maybeInvoke<VideoAsset>("analyze_input", payload);
}

export async function createDownloadTask(
  payload: CreateTaskPayload
): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    return {
      id: `task-${Date.now()}`,
      platform: payload.platform,
      title: payload.title,
      progress: 100,
      speedText: "-",
      formatLabel: payload.formatLabel ?? "视频",
      status: "completed",
      etaText: "已完成",
      message: "浏览器预览模式使用模拟下载结果",
      outputPath: `${payload.saveDirectoryOverride ?? mockState.saveDirectory}/${payload.title}`,
      supportsPause: false,
      supportsCancel: false
    };
  }

  return maybeInvoke<DownloadTask>("create_download_task", payload);
}

export async function analyzeProfileInput(
  payload: AnalyzeProfilePayload
): Promise<ProfileBatch> {
  if (!hasTauriRuntime()) {
    return {
      profileTitle: "示例主页",
      sourceUrl: payload.rawInput,
      totalAvailable: payload.limit ?? 12,
      fetchedCount: payload.limit ?? 12,
      skippedCount: 0,
      items: Array.from({ length: payload.limit ?? 12 }).map((_, index) => ({
        ...mockState.preview,
        assetId: `${mockState.preview.assetId}-${index + 1}`,
        title: `${mockState.preview.title} ${index + 1}`,
        sourceUrl: `${mockState.preview.sourceUrl}?item=${index + 1}`
      }))
    };
  }

  return maybeInvoke<ProfileBatch>("analyze_profile_input", payload);
}

export async function createProfileDownloadTasks(
  payload: CreateProfileDownloadTasksPayload
): Promise<BatchDownloadResult> {
  if (!hasTauriRuntime()) {
    return {
      profileTitle: "示例主页",
      sourceUrl: payload.sourceUrl,
      totalAvailable: payload.items.length,
      fetchedCount: payload.items.length,
      enqueuedCount: payload.items.length,
      skippedCount: 0,
      message: "浏览器预览模式使用模拟批量入队结果"
    };
  }

  return maybeInvoke<BatchDownloadResult>("create_profile_download_tasks", payload);
}

export async function listDownloadTasks(): Promise<DownloadTask[]> {
  if (!hasTauriRuntime()) {
    return mockState.tasks;
  }

  return maybeInvoke<DownloadTask[]>("list_download_tasks");
}

export async function saveSettings(
  payload: SaveSettingsPayload
): Promise<SettingsProfile> {
  if (!hasTauriRuntime()) {
    return {
      authState: payload.cookieBrowser ? "active" : "guest",
      accountLabel: payload.cookieBrowser
        ? `浏览器 Cookie · ${payload.cookieBrowser}`
        : "未登录",
      cookieBrowser: payload.cookieBrowser,
      saveDirectory: payload.saveDirectory,
      downloadMode: payload.downloadMode,
      qualityPreference: payload.qualityPreference,
      autoRevealInFinder: payload.autoRevealInFinder,
      ffmpegAvailable: false
    };
  }

  return maybeInvoke<SettingsProfile>("save_settings", payload);
}

export async function pickSaveDirectory(
  currentDirectory: string | null
): Promise<string | null> {
  if (!hasTauriRuntime()) {
    return currentDirectory ?? mockState.saveDirectory;
  }

  return maybeInvoke<string | null>("pick_save_directory", { currentDirectory });
}

export async function openInFileManager(
  path: string,
  revealParent = false
): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  return maybeInvoke<void>("open_in_file_manager", { path, revealParent });
}

export async function clearFinishedTasks(): Promise<DownloadTask[]> {
  if (!hasTauriRuntime()) {
    return mockState.tasks.filter(
      (task) =>
        task.status !== "completed" &&
        task.status !== "failed" &&
        task.status !== "cancelled"
    );
  }

  return maybeInvoke<DownloadTask[]>("clear_finished_tasks");
}

export async function pauseDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("浏览器预览模式不支持暂停任务。");
  }

  return maybeInvoke<DownloadTask>("pause_download_task", { taskId });
}

export async function resumeDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("浏览器预览模式不支持继续任务。");
  }

  return maybeInvoke<DownloadTask>("resume_download_task", { taskId });
}

export async function cancelDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("浏览器预览模式不支持取消任务。");
  }

  return maybeInvoke<DownloadTask>("cancel_download_task", { taskId });
}
