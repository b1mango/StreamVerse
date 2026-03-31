import { mockState } from "./mock";
import type {
  AnalyzeInputPayload,
  AnalysisProgress,
  AnalyzeProfilePayload,
  BatchItemSelection,
  BatchDownloadResult,
  BrowserLaunchResult,
  BootstrapState,
  CreateProfileDownloadTasksPayload,
  CreateTaskPayload,
  DownloadTask,
  ProfileBatch,
  SaveSettingsPayload,
  SetModuleEnabledPayload,
  SettingsProfile,
  ModuleRuntimeState,
  ModuleId,
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

export async function getAnalysisProgress(
  sessionId: string
): Promise<AnalysisProgress | null> {
  if (!hasTauriRuntime()) {
    return null;
  }

  return maybeInvoke<AnalysisProgress | null>("get_analysis_progress", { sessionId });
}

export async function clearAnalysisProgress(sessionId: string): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  await maybeInvoke<void>("clear_analysis_progress", { sessionId });
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
      message: "下载任务已完成",
      outputPath: `${payload.saveDirectoryOverride ?? mockState.saveDirectory}/${payload.title}`,
      supportsPause: false,
      supportsCancel: false,
      canRetry: true
    };
  }

  return maybeInvoke<DownloadTask>("create_download_task", payload);
}

export async function analyzeProfileInput(
  payload: AnalyzeProfilePayload
): Promise<ProfileBatch> {
  if (!hasTauriRuntime()) {
    const isBilibili = /bilibili|b23\.tv/i.test(payload.rawInput);
    const preview = {
      ...mockState.preview,
      platform: isBilibili ? "bilibili" : "douyin",
      sourceUrl: payload.rawInput
    } as VideoAsset;

    return {
      profileTitle: isBilibili ? "示例 UP 主" : "示例主页",
      sourceUrl: payload.rawInput,
      totalAvailable: payload.limit ?? 12,
      fetchedCount: payload.limit ?? 12,
      skippedCount: 0,
      items: Array.from({ length: payload.limit ?? 12 }).map((_, index) => ({
        ...preview,
        assetId: `${preview.assetId}-${index + 1}`,
        title: `${preview.title} ${index + 1}`,
        sourceUrl: `${preview.sourceUrl}?item=${index + 1}`
      }))
    };
  }

  return maybeInvoke<ProfileBatch>("analyze_profile_input", payload);
}

export async function openProfileBrowser(
  payload: AnalyzeProfilePayload
): Promise<BrowserLaunchResult> {
  if (!hasTauriRuntime()) {
    return {
      port: 9222,
      browser: "chrome"
    };
  }

  return maybeInvoke<BrowserLaunchResult>("open_profile_browser", payload);
}

export async function collectProfileBrowser(
  payload: AnalyzeProfilePayload & { port: number }
): Promise<ProfileBatch> {
  if (!hasTauriRuntime()) {
    const preview = {
      ...mockState.preview,
      platform: "douyin",
      sourceUrl: payload.rawInput,
      formats: []
    } as VideoAsset;

    return {
      profileTitle: "示例主页",
      sourceUrl: payload.rawInput,
      totalAvailable: 12,
      fetchedCount: 12,
      skippedCount: 0,
      sessionCookieFile: null,
      items: Array.from({ length: 12 }).map((_, index) => ({
        ...preview,
        assetId: `${preview.assetId}-${index + 1}`,
        title: `${preview.title} ${index + 1}`,
        sourceUrl: `${preview.sourceUrl}?item=${index + 1}`
      }))
    };
  }

  return maybeInvoke<ProfileBatch>("collect_profile_browser", payload);
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
      message: "已加入队列"
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

export async function setModuleEnabled(
  payload: SetModuleEnabledPayload
): Promise<ModuleRuntimeState[]> {
  if (!hasTauriRuntime()) {
    return mockState.modules.map((module) =>
      module.id === payload.moduleId ? { ...module, enabled: payload.enabled } : module
    );
  }

  return maybeInvoke<ModuleRuntimeState[]>("set_module_enabled", payload);
}

function applyMockModulePack(
  moduleId: ModuleId,
  installed: boolean
): ModuleRuntimeState[] {
  const sharedIds =
    moduleId === "douyin-single" || moduleId === "douyin-profile"
      ? ["douyin-single", "douyin-profile"]
      : moduleId === "bilibili-single" || moduleId === "bilibili-profile"
        ? ["bilibili-single", "bilibili-profile"]
        : ["youtube-single"];

  return mockState.modules.map((module) =>
    sharedIds.includes(module.id)
      ? { ...module, installed, enabled: installed ? true : false }
      : module
  );
}

export async function installModulePack(
  moduleId: ModuleId
): Promise<ModuleRuntimeState[]> {
  if (!hasTauriRuntime()) {
    return applyMockModulePack(moduleId, true);
  }

  return maybeInvoke<ModuleRuntimeState[]>("install_module_pack", { moduleId });
}

export async function uninstallModulePack(
  moduleId: ModuleId
): Promise<ModuleRuntimeState[]> {
  if (!hasTauriRuntime()) {
    return applyMockModulePack(moduleId, false);
  }

  return maybeInvoke<ModuleRuntimeState[]>("uninstall_module_pack", { moduleId });
}

export async function updateModulePack(
  moduleId: ModuleId
): Promise<ModuleRuntimeState[]> {
  if (!hasTauriRuntime()) {
    return applyMockModulePack(moduleId, true).map((module) =>
      module.id === moduleId
        ? {
            ...module,
            installed: true,
            enabled: true,
            currentVersion: module.latestVersion ?? "0.1.0",
            updateAvailable: false
          }
        : module
    );
  }

  return maybeInvoke<ModuleRuntimeState[]>("update_module_pack", { moduleId });
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

export async function installDownloadEngine(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  return maybeInvoke<void>("install_download_engine");
}

export async function pauseDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("当前环境不支持暂停任务。");
  }

  return maybeInvoke<DownloadTask>("pause_download_task", { taskId });
}

export async function resumeDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("当前环境不支持继续任务。");
  }

  return maybeInvoke<DownloadTask>("resume_download_task", { taskId });
}

export async function cancelDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    throw new Error("当前环境不支持取消任务。");
  }

  return maybeInvoke<DownloadTask>("cancel_download_task", { taskId });
}

export async function retryDownloadTask(taskId: string): Promise<DownloadTask> {
  if (!hasTauriRuntime()) {
    return {
      id: taskId,
      platform: "douyin",
      title: "重试任务",
      progress: 0,
      speedText: "-",
      formatLabel: "视频",
      status: "queued",
      etaText: "等待中",
      message: "任务已重新加入队列",
      outputPath: undefined,
      supportsPause: false,
      supportsCancel: false,
      canRetry: true
    };
  }

  return maybeInvoke<DownloadTask>("retry_download_task", { taskId });
}
