<script lang="ts">
  import { onMount } from "svelte";
  import PlatformHome from "./lib/components/PlatformHome.svelte";
  import ProfileBatchWorkspace from "./lib/components/ProfileBatchWorkspace.svelte";
  import SettingsPanel from "./lib/components/SettingsPanel.svelte";
  import SharedDirectoryBar from "./lib/components/SharedDirectoryBar.svelte";
  import SingleVideoWorkspace from "./lib/components/SingleVideoWorkspace.svelte";
  import TaskQueuePanel from "./lib/components/TaskQueuePanel.svelte";
  import { setLanguage, t, tRaw } from "./lib/i18n";
  import {
    analyzeInput,
    analyzeProfileInput,
    cancelDownloadTask,
    checkDownloadHistory,
    clearAnalysisProgress,
    collectProfileBrowser,
    clearFinishedTasks,
    createDownloadTask,
    createProfileDownloadTasks,
    getAnalysisProgress,
    getBootstrapState,
    listDownloadTasks,
    openInFileManager,
    openProfileBrowser,
    pauseDownloadTask,
    pickCookieFile,
    pickSaveDirectory,
    retryDownloadTask,
    resumeDownloadTask,
    saveSettings
  } from "./lib/backend";
  import {
    clampBatchLimit,
    createDefaultDownloadOptions,
    hasSelectedDownloadOptions,
    pickPreferredFormat,
    resolveErrorMessage,
    selectedFormat
  } from "./lib/media";
  import {
    authMap,
    browserOptions,
    moduleCatalog,
    moduleOrder,
    platformMeta,
    qualityOptions
  } from "./lib/options";
  import type {
    AnalysisProgress,
    BootstrapState,
    DownloadContentSelection,
    DownloadMode,
    DownloadTask,
    LanguageCode,
    ModuleId,
    ModuleRuntimeState,
    PlatformAuthDraft,
    PlatformAuthProfile,
    PlatformId,
    ProfileBatch,
    BrowserLaunchResult,
    QualityPreference,
    SettingsProfile,
    ThemeMode,
    VideoAsset,
    VideoFormat
  } from "./lib/types";

  let loading = true;
  let settingsOpen = false;
  let settingsSaving = false;
  let pickingDirectory = false;
  let pickingCookieFilePlatform: PlatformId | null = null;
  let pickingTargetDirectory = false;
  let openingFolder = false;
  let clearingFinished = false;
  let activeModule: ModuleId | null = null;
  let bootstrap: BootstrapState | null = null;
  let moduleStates: ModuleRuntimeState[] = [];
  let tasks: DownloadTask[] = [];
  let pollTimer: number | undefined;
  let pollingTasks = false;

  let analysisModalOpen = false;
  let analysisModalProgress: AnalysisProgress | null = null;
  let analysisModalLabel = "";
  let analysisModalDone = false;

  let douyinSingleInput = "";
  let douyinSinglePreview: VideoAsset | null = null;
  let douyinSelectedFormatId = "";
  let douyinSingleOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingDouyinSingle = false;
  let douyinSingleAnalysisProgress: AnalysisProgress | null = null;
  let downloadingDouyinSingle = false;
  let pastingDouyinSingle = false;

  let douyinProfileInput = "";
  let douyinProfilePreview: ProfileBatch | null = null;
  let douyinSelectedProfileIds: string[] = [];
  let douyinSelectedProfileFormatIds: Record<string, string> = {};
  let douyinProfileOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingDouyinProfile = false;
  let douyinProfileAnalysisProgress: AnalysisProgress | null = null;
  let openingDouyinProfileBrowser = false;
  let enqueuingDouyinProfile = false;
  let pastingDouyinProfile = false;
  let douyinProfileBrowserSession: BrowserLaunchResult | null = null;
  let douyinDownloadedAssetIds: string[] = [];

  let bilibiliInput = "";
  let bilibiliPreview: VideoAsset | null = null;
  let bilibiliSelectedFormatId = "";
  let bilibiliOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingBilibili = false;
  let bilibiliAnalysisProgress: AnalysisProgress | null = null;
  let downloadingBilibili = false;
  let pastingBilibili = false;
  let bilibiliProfileInput = "";
  let bilibiliProfilePreview: ProfileBatch | null = null;
  let bilibiliSelectedProfileIds: string[] = [];
  let bilibiliSelectedProfileFormatIds: Record<string, string> = {};
  let bilibiliProfileOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingBilibiliProfile = false;
  let bilibiliProfileAnalysisProgress: AnalysisProgress | null = null;
  let enqueuingBilibiliProfile = false;
  let pastingBilibiliProfile = false;
  let bilibiliDownloadedAssetIds: string[] = [];

  let youtubeInput = "";
  let youtubePreview: VideoAsset | null = null;
  let youtubeSelectedFormatId = "";
  let youtubeOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingYoutube = false;
  let youtubeAnalysisProgress: AnalysisProgress | null = null;
  let downloadingYoutube = false;
  let pastingYoutube = false;

  let errorMessage = "";
  let successMessage = "";
  let platformAuthDrafts: Record<PlatformId, PlatformAuthDraft> = createEmptyPlatformAuthDrafts();
  let isWindowsPlatform = false;
  let saveDirectoryDraft = "";
  let targetDirectory = "";
  let downloadMode: DownloadMode = "manual";
  let qualityPreference: QualityPreference = "recommended";
  let autoRevealInFinder = false;
  let maxConcurrentDownloads = 3;
  let proxyUrl = "";
  let speedLimit = "";
  let autoUpdate = false;
  let theme: ThemeMode = "dark";
  let notifyOnComplete = true;
  let language: LanguageCode = "zh-CN";
  let taskActionPendingIds: string[] = [];

  function createEmptyPlatformAuthDrafts(): Record<PlatformId, PlatformAuthDraft> {
    return {
      douyin: { cookieBrowser: null, cookieFile: null, cookieText: null },
      bilibili: { cookieBrowser: null, cookieFile: null, cookieText: null },
      youtube: { cookieBrowser: null, cookieFile: null, cookieText: null }
    };
  }

  function clonePlatformAuthDrafts(
    platformAuth?: Record<PlatformId, PlatformAuthProfile>
  ): Record<PlatformId, PlatformAuthDraft> {
    return {
      douyin: {
        cookieBrowser: platformAuth?.douyin?.cookieBrowser ?? null,
        cookieFile: platformAuth?.douyin?.cookieFile ?? null,
        cookieText: null
      },
      bilibili: {
        cookieBrowser: platformAuth?.bilibili?.cookieBrowser ?? null,
        cookieFile: platformAuth?.bilibili?.cookieFile ?? null,
        cookieText: null
      },
      youtube: {
        cookieBrowser: platformAuth?.youtube?.cookieBrowser ?? null,
        cookieFile: platformAuth?.youtube?.cookieFile ?? null,
        cookieText: null
      }
    };
  }

  function authProfileFor(platform: PlatformId): PlatformAuthProfile {
    return (
      bootstrap?.platformAuth?.[platform] ?? {
        authState: "guest",
        accountLabel: "未登录",
        cookieBrowser: null,
        cookieFile: null
      }
    );
  }

  function authStateFor(platform: PlatformId) {
    return authProfileFor(platform).authState;
  }

  function cookieFileFor(platform: PlatformId) {
    return authProfileFor(platform).cookieFile;
  }

  import {
    isPermissionGranted,
    requestPermission,
    sendNotification
  } from "@tauri-apps/plugin-notification";

  const isDesktopRuntime =
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  let completedTaskIds = new Set<string>();

  async function notifyIfNewCompletions(newTasks: DownloadTask[]) {
    if (!notifyOnComplete || !isDesktopRuntime) return;
    const newlyCompleted = newTasks.filter(
      (t) => t.status === "completed" && !completedTaskIds.has(t.id)
    );
    if (!newlyCompleted.length) return;

    for (const t of newlyCompleted) {
      completedTaskIds.add(t.id);
    }

    let permissionGranted = await isPermissionGranted();
    if (!permissionGranted) {
      const permission = await requestPermission();
      permissionGranted = permission === "granted";
    }
    if (!permissionGranted) return;

    if (newlyCompleted.length === 1) {
      sendNotification({
        title: tRaw("notify.downloadComplete"),
        body: newlyCompleted[0].title ?? tRaw("notify.taskCompleted")
      });
    } else {
      sendNotification({
        title: tRaw("notify.downloadComplete"),
        body: `${newlyCompleted.length} ${tRaw("notify.tasksCompleted")}`
      });
    }
  }

  onMount(() => {
    void initialize();

    return () => {
      if (pollTimer) {
        window.clearInterval(pollTimer);
      }
    };
  });

  async function initialize() {
    bootstrap = await getBootstrapState();
    tasks = bootstrap.tasks;
    moduleStates = bootstrap.modules;
    syncSettings(bootstrap);

    // Seed completed IDs so we don't notify for already-completed tasks on load
    for (const t of tasks) {
      if (t.status === "completed") {
        completedTaskIds.add(t.id);
      }
    }

    if (!isDesktopRuntime) {
      douyinSinglePreview = bootstrap.preview;
      douyinSingleInput = bootstrap.preview.sourceUrl;
      douyinSelectedFormatId =
        pickPreferredFormat(
          bootstrap.preview,
          bootstrap.qualityPreference,
          authStateFor(bootstrap.preview.platform as PlatformId)
        )?.id ?? "";
    }

    if (isDesktopRuntime) {
      pollTimer = window.setInterval(async () => {
        if (pollingTasks) {
          return;
        }

        pollingTasks = true;
        try {
          const freshTasks = await listDownloadTasks();
          void notifyIfNewCompletions(freshTasks);
          tasks = freshTasks;
        } finally {
          pollingTasks = false;
        }
      }, 280);
    }

    loading = false;
  }

  function syncSettings(next: BootstrapState | SettingsProfile) {
    if ("isWindows" in next) {
      isWindowsPlatform = next.isWindows;
    }
    platformAuthDrafts = clonePlatformAuthDrafts(next.platformAuth);
    saveDirectoryDraft = next.saveDirectory;
    targetDirectory = next.saveDirectory;
    downloadMode = next.downloadMode;
    qualityPreference = next.qualityPreference;
    autoRevealInFinder = next.autoRevealInFinder;
    maxConcurrentDownloads = next.maxConcurrentDownloads;
    proxyUrl = next.proxyUrl ?? "";
    speedLimit = next.speedLimit ?? "";
    autoUpdate = next.autoUpdate;
    theme = next.theme;
    notifyOnComplete = next.notifyOnComplete;
    language = next.language;
    applyTheme(next.theme);
    setLanguage(next.language);
  }

  function applyTheme(mode: ThemeMode) {
    const root = document.documentElement;
    root.classList.add("no-transition");
    root.setAttribute("data-theme", mode);
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        root.classList.remove("no-transition");
      });
    });
  }

  function clearNotices() {
    errorMessage = "";
    successMessage = "";
  }

  function backToPlatformHome() {
    activeModule = null;
    clearNotices();
  }

  function openModule(moduleId: ModuleId) {
    activeModule = moduleId;
    clearNotices();
  }

  function moduleEnabled(moduleId: ModuleId) {
    return moduleStates.find((module) => module.id === moduleId)?.installed ?? true;
  }

  function buildBatchFormatSelections(items: VideoAsset[]) {
    return Object.fromEntries(
      items.map((item) => [
        item.assetId,
        pickPreferredFormat(item, qualityPreference, authStateFor(item.platform as PlatformId))?.id ?? ""
      ])
    );
  }

  async function analyzeSinglePreview(
    platform: PlatformId,
    rawInput: string,
    sessionId?: string
  ): Promise<VideoAsset> {
    const preview = await analyzeInput({ rawInput, sessionId });
    if (preview.platform !== platform) {
      throw new Error(`请使用 ${platformMeta[platform].label} 链接。`);
    }

    return preview;
  }

  function createAnalysisSessionId() {
    return `analysis-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  }

  function normalizeAnalysisProgress(progress: AnalysisProgress): AnalysisProgress {
    const current = Math.max(0, Math.round(progress.current || 0));
    const total = Math.max(current, Math.round(progress.total || 0));
    return {
      current,
      total,
      message: progress.message?.trim() || "正在解析…"
    };
  }

  async function withAnalysisProgress<T>(
    setProgress: (progress: AnalysisProgress | null) => void,
    runner: (sessionId: string) => Promise<T>,
    modalLabel?: string,
    onResult?: (result: T) => void | Promise<void>
  ): Promise<T> {
    const sessionId = createAnalysisSessionId();
    let pollId: number | undefined;
    const initialProgress: AnalysisProgress = { current: 0, total: 0, message: "准备解析…" };
    setProgress(initialProgress);

    if (modalLabel) {
      analysisModalLabel = modalLabel;
      analysisModalProgress = initialProgress;
      analysisModalOpen = true;
    }

    if (isDesktopRuntime) {
      pollId = window.setInterval(async () => {
        try {
          const next = await getAnalysisProgress(sessionId);
          if (next) {
            const normalized = normalizeAnalysisProgress(next);
            setProgress(normalized);
            if (modalLabel) analysisModalProgress = normalized;
          }
        } catch {}
      }, 180);
    }

    let result: T;
    try {
      result = await runner(sessionId);
    } catch (error) {
      if (pollId) window.clearInterval(pollId);
      if (isDesktopRuntime) {
        try { await clearAnalysisProgress(sessionId); } catch {}
      }
      if (modalLabel) {
        analysisModalOpen = false;
        analysisModalProgress = null;
        analysisModalDone = false;
      }
      throw error;
    }

    if (pollId) {
      window.clearInterval(pollId);
    }
    if (isDesktopRuntime) {
      try {
        const finalProgress = await getAnalysisProgress(sessionId);
        if (finalProgress) {
          const normalized = normalizeAnalysisProgress(finalProgress);
          setProgress(normalized);
          if (modalLabel) {
            analysisModalProgress = normalized;
            analysisModalDone = true;
          }
        }
      } catch {}
    }

    if (onResult) {
      await new Promise((r) => setTimeout(r, 200));
      await onResult(result);
      await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 60)));
    }

    if (modalLabel && analysisModalDone) {
      await new Promise((r) => setTimeout(r, 800));
    }

    if (isDesktopRuntime) {
      try { await clearAnalysisProgress(sessionId); } catch {}
    }
    if (modalLabel) {
      analysisModalOpen = false;
      analysisModalProgress = null;
      analysisModalDone = false;
    }

    return result;
  }

  async function handleAnalyzeDouyinSingle() {
    analyzingDouyinSingle = true;
    clearNotices();

    try {
      const preview = await withAnalysisProgress(
        (progress) => (douyinSingleAnalysisProgress = progress),
        (sessionId) => analyzeSinglePreview("douyin", douyinSingleInput, sessionId)
      );
      douyinSinglePreview = preview;
      douyinSelectedFormatId =
        pickPreferredFormat(preview, qualityPreference, authStateFor("douyin"))?.id ?? "";

      successMessage = "抖音链接已解析。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingDouyinSingle = false;
    }
  }

  async function handleAnalyzeBilibiliSingle() {
    analyzingBilibili = true;
    clearNotices();

    try {
      const preview = await withAnalysisProgress(
        (progress) => (bilibiliAnalysisProgress = progress),
        (sessionId) => analyzeSinglePreview("bilibili", bilibiliInput, sessionId)
      );
      bilibiliPreview = preview;
      bilibiliSelectedFormatId =
        pickPreferredFormat(preview, qualityPreference, authStateFor("bilibili"))?.id ?? "";

      successMessage = "Bilibili 链接已解析。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingBilibili = false;
    }
  }

  async function handleAnalyzeYoutubeSingle() {
    analyzingYoutube = true;
    clearNotices();

    try {
      const preview = await withAnalysisProgress(
        (progress) => (youtubeAnalysisProgress = progress),
        (sessionId) => analyzeSinglePreview("youtube", youtubeInput, sessionId)
      );
      youtubePreview = preview;
      youtubeSelectedFormatId =
        pickPreferredFormat(preview, qualityPreference, authStateFor("youtube"))?.id ?? "";

      successMessage = "YouTube 链接已解析。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingYoutube = false;
    }
  }

  async function handleAnalyzeDouyinProfile() {
    analyzingDouyinProfile = true;
    clearNotices();

    try {
      douyinProfilePreview = null;
      douyinSelectedProfileIds = [];
      douyinSelectedProfileFormatIds = {};

      await withAnalysisProgress(
        (progress) => (douyinProfileAnalysisProgress = progress),
        (sessionId) => {
          if (cookieFileFor("douyin")) {
            return analyzeProfileInput({
              rawInput: douyinProfileInput,
              sessionId
            });
          }

          if (!douyinProfileBrowserSession) {
            throw new Error("首次使用请先在设置里导入登录 Cookie；如果还没导入，也可以先打开浏览器登录后再点“读取完整列表”。");
          }

          return collectProfileBrowser({
            rawInput: douyinProfileInput,
            port: douyinProfileBrowserSession.port,
            sessionId
          });
        },
        "正在读取抖音主页作品…",
        async (r) => {
          douyinProfilePreview = r;
          douyinSelectedProfileIds = r.items.map((item) => item.assetId);
          douyinSelectedProfileFormatIds = buildBatchFormatSelections(r.items);
          successMessage = `已读取 ${r.fetchedCount} 个作品。`;
          try {
            douyinDownloadedAssetIds = await checkDownloadHistory(
              "douyin",
              r.items.map((item) => item.assetId)
            );
          } catch {
            douyinDownloadedAssetIds = [];
          }
        }
      );
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingDouyinProfile = false;
    }
  }

  async function handleOpenDouyinProfileBrowser() {
    openingDouyinProfileBrowser = true;
    clearNotices();

    try {
      douyinProfilePreview = null;
      douyinSelectedProfileIds = [];
      douyinSelectedProfileFormatIds = {};
      douyinProfileBrowserSession = await openProfileBrowser({
        rawInput: douyinProfileInput
      });
      successMessage = "浏览器已打开。登录后回到这里点“读取完整列表”。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      openingDouyinProfileBrowser = false;
    }
  }

  async function handleAnalyzeBilibiliProfile() {
    analyzingBilibiliProfile = true;
    clearNotices();

    try {
      bilibiliProfilePreview = null;
      bilibiliSelectedProfileIds = [];
      bilibiliSelectedProfileFormatIds = {};
      await withAnalysisProgress(
        (progress) => (bilibiliProfileAnalysisProgress = progress),
        (sessionId) =>
          analyzeProfileInput({
            rawInput: bilibiliProfileInput,
            sessionId
          }),
        "正在读取 Bilibili 主页视频…",
        async (r) => {
          if (r.items.some((item) => item.platform !== "bilibili")) {
            throw new Error("请使用 Bilibili UP 主空间页链接。");
          }

          bilibiliProfilePreview = r;
          bilibiliSelectedProfileIds = r.items.map((item) => item.assetId);
          bilibiliSelectedProfileFormatIds = buildBatchFormatSelections(r.items);
          successMessage = `已读取 ${r.fetchedCount} 个视频。`;
          try {
            bilibiliDownloadedAssetIds = await checkDownloadHistory(
              "bilibili",
              r.items.map((item) => item.assetId)
            );
          } catch {
            bilibiliDownloadedAssetIds = [];
          }
        }
      );
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingBilibiliProfile = false;
    }
  }

  async function pasteAndAnalyze(platform: PlatformId, kind: "single" | "profile") {
    if (platform === "douyin" && kind === "single") {
      pastingDouyinSingle = true;
    } else if (platform === "douyin") {
      pastingDouyinProfile = true;
    } else if (platform === "bilibili" && kind === "single") {
      pastingBilibili = true;
    } else if (platform === "youtube") {
      pastingYoutube = true;
    } else {
      pastingBilibiliProfile = true;
    }

    clearNotices();

    try {
      if (typeof navigator === "undefined" || !navigator.clipboard?.readText) {
        throw new Error("当前环境不支持直接读取剪贴板。");
      }

      const text = (await navigator.clipboard.readText()).trim();
      if (!text) {
        throw new Error("剪贴板里没有可解析的内容。");
      }

      if (platform === "douyin" && kind === "single") {
        douyinSingleInput = text;
        await handleAnalyzeDouyinSingle();
      } else if (platform === "douyin") {
        douyinProfileInput = text;
        await handleAnalyzeDouyinProfile();
      } else if (platform === "bilibili" && kind === "single") {
        bilibiliInput = text;
        await handleAnalyzeBilibiliSingle();
      } else if (platform === "youtube") {
        youtubeInput = text;
        await handleAnalyzeYoutubeSingle();
      } else {
        bilibiliProfileInput = text;
        await handleAnalyzeBilibiliProfile();
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      if (platform === "douyin" && kind === "single") {
        pastingDouyinSingle = false;
      } else if (platform === "douyin") {
        pastingDouyinProfile = false;
      } else if (platform === "bilibili" && kind === "single") {
        pastingBilibili = false;
      } else if (platform === "youtube") {
        pastingYoutube = false;
      } else {
        pastingBilibiliProfile = false;
      }
    }
  }

  async function handlePasteDouyinProfileInput() {
    pastingDouyinProfile = true;
    clearNotices();

    try {
      if (typeof navigator === "undefined" || !navigator.clipboard?.readText) {
        throw new Error("当前环境不支持直接读取剪贴板。");
      }

      const text = (await navigator.clipboard.readText()).trim();
      if (!text) {
        throw new Error("剪贴板里没有可解析的内容。");
      }

      douyinProfileInput = text;
      douyinProfilePreview = null;
      douyinSelectedProfileIds = [];
      douyinSelectedProfileFormatIds = {};
      douyinProfileBrowserSession = null;
      successMessage = "主页链接已填入。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      pastingDouyinProfile = false;
    }
  }

  async function startSingleDownload(
    platform: PlatformId,
    asset: VideoAsset,
    selectedFormatId: string,
    downloadOptions: DownloadContentSelection,
    launchedBySmartMode: boolean
  ) {
    const setLoading = (value: boolean) => {
      if (platform === "douyin") {
        downloadingDouyinSingle = value;
      } else if (platform === "bilibili") {
        downloadingBilibili = value;
      } else {
        downloadingYoutube = value;
      }
    };

    setLoading(true);
    clearNotices();

    try {
      if (!hasSelectedDownloadOptions(downloadOptions)) {
        throw new Error("至少要选择一种要保存的内容。");
      }

      const format = downloadOptions.downloadVideo
        ? selectedFormat(asset, selectedFormatId, authStateFor(platform))
        : undefined;

      if (downloadOptions.downloadVideo && !format) {
        throw new Error("请先选择一个可用清晰度。");
      }

      const task = await createDownloadTask({
        assetId: asset.assetId,
        platform: asset.platform,
        sourceUrl: asset.sourceUrl,
        title: asset.title,
        author: asset.author,
        publishDate: asset.publishDate,
        caption: asset.caption,
        coverUrl: asset.coverUrl ?? null,
        formatId: format?.id ?? null,
        formatLabel: format?.label ?? null,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions,
        directUrl: format?.directUrl ?? null,
        referer: format?.referer ?? null,
        userAgent: format?.userAgent ?? null,
        audioDirectUrl: format?.audioDirectUrl ?? null,
        audioReferer: format?.audioReferer ?? null,
        audioUserAgent: format?.audioUserAgent ?? null
      });

      upsertTask(task);
      successMessage = launchedBySmartMode
        ? "已创建下载任务。"
        : task.message ?? "下载任务已开始。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      setLoading(false);
    }
  }

  async function handleEnqueueDouyinProfileTasks() {
    if (!douyinProfilePreview) {
      return;
    }

    const items = douyinProfilePreview.items.filter((item) =>
      douyinSelectedProfileIds.includes(item.assetId)
    );

    if (!items.length) {
      errorMessage = "请至少勾选一个主页作品。";
      return;
    }

    if (!hasSelectedDownloadOptions(douyinProfileOptions)) {
      errorMessage = "至少要选择一种要保存的内容。";
      return;
    }

    enqueuingDouyinProfile = true;
    clearNotices();

    try {
      const result = await createProfileDownloadTasks({
        profileTitle: douyinProfilePreview.profileTitle,
        sourceUrl: douyinProfilePreview.sourceUrl,
        items: items.map((asset) => ({
          asset,
          selectedFormatId: douyinSelectedProfileFormatIds[asset.assetId] ?? null
        })),
        sessionCookieFile: douyinProfilePreview.sessionCookieFile ?? null,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions: douyinProfileOptions
      });
      successMessage = result.message;
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      enqueuingDouyinProfile = false;
    }
  }

  async function handleEnqueueBilibiliProfileTasks() {
    if (!bilibiliProfilePreview) {
      return;
    }

    const items = bilibiliProfilePreview.items.filter((item) =>
      bilibiliSelectedProfileIds.includes(item.assetId)
    );

    if (!items.length) {
      errorMessage = "请至少勾选一个 UP 主视频。";
      return;
    }

    if (!hasSelectedDownloadOptions(bilibiliProfileOptions)) {
      errorMessage = "至少要选择一种要保存的内容。";
      return;
    }

    enqueuingBilibiliProfile = true;
    clearNotices();

    try {
      const result = await createProfileDownloadTasks({
        profileTitle: bilibiliProfilePreview.profileTitle,
        sourceUrl: bilibiliProfilePreview.sourceUrl,
        items: items.map((asset) => ({
          asset,
          selectedFormatId: bilibiliSelectedProfileFormatIds[asset.assetId] ?? null
        })),
        sessionCookieFile: bilibiliProfilePreview.sessionCookieFile ?? null,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions: bilibiliProfileOptions
      });
      successMessage = result.message;
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      enqueuingBilibiliProfile = false;
    }
  }

  async function handleTaskControl(
    task: DownloadTask,
    action: "pause" | "resume" | "cancel" | "retry"
  ) {
    taskActionPendingIds = [...taskActionPendingIds, task.id];
    errorMessage = "";

    try {
      const nextTask =
        action === "pause"
          ? await pauseDownloadTask(task.id)
          : action === "resume"
            ? await resumeDownloadTask(task.id)
            : action === "retry"
              ? await retryDownloadTask(task.id)
              : await cancelDownloadTask(task.id);
      upsertTask(nextTask);
      if (action === "retry") {
        successMessage = "任务已重新加入队列。";
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      taskActionPendingIds = taskActionPendingIds.filter((id) => id !== task.id);
    }
  }

  async function handlePickSaveDirectory() {
    if (!bootstrap) {
      return;
    }

    pickingDirectory = true;
    errorMessage = "";

    try {
      const pickedDirectory = await pickSaveDirectory(
        saveDirectoryDraft || bootstrap.saveDirectory
      );
      if (pickedDirectory) {
        saveDirectoryDraft = pickedDirectory;
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      pickingDirectory = false;
    }
  }

  async function handlePickTargetDirectory() {
    if (!bootstrap) {
      return;
    }

    pickingTargetDirectory = true;
    errorMessage = "";

    try {
      const pickedDirectory = await pickSaveDirectory(resolvedTargetDirectory());
      if (pickedDirectory) {
        targetDirectory = pickedDirectory;
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      pickingTargetDirectory = false;
    }
  }

  async function handlePickCookieFile(platform: PlatformId) {
    pickingCookieFilePlatform = platform;
    errorMessage = "";

    try {
      const pickedFile = await pickCookieFile(platformAuthDrafts[platform].cookieFile || null);
      if (pickedFile) {
        platformAuthDrafts = {
          ...platformAuthDrafts,
          [platform]: {
            ...platformAuthDrafts[platform],
            cookieFile: pickedFile
          }
        };
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      pickingCookieFilePlatform = null;
    }
  }

  async function handleOpenCurrentDirectory() {
    const path = resolvedTargetDirectory();
    if (!path) {
      return;
    }

    openingFolder = true;
    errorMessage = "";

    try {
      await openInFileManager(path, false);
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      openingFolder = false;
    }
  }

  async function handleRevealTask(task: DownloadTask) {
    if (!task.outputPath) {
      return;
    }

    errorMessage = "";

    try {
      await openInFileManager(task.outputPath, true);
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    }
  }

  async function handleClearFinished() {
    clearingFinished = true;
    errorMessage = "";

    try {
      tasks = await clearFinishedTasks();
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      clearingFinished = false;
    }
  }

  async function handleSaveSettings() {
    if (!bootstrap) {
      return;
    }

    clearNotices();
    settingsSaving = true;

    // Close panel immediately — visual settings (theme, language) are already previewed
    settingsOpen = false;

    try {
      const nextSettings = await saveSettings({
        platformAuth: platformAuthDrafts,
        saveDirectory: saveDirectoryDraft,
        downloadMode,
        qualityPreference,
        autoRevealInFinder,
        maxConcurrentDownloads,
        proxyUrl: proxyUrl || null,
        speedLimit: speedLimit || null,
        autoUpdate,
        theme,
        notifyOnComplete,
        language
      });

      bootstrap = {
        ...bootstrap,
        authState: nextSettings.authState,
        accountLabel: nextSettings.accountLabel,
        platformAuth: nextSettings.platformAuth,
        saveDirectory: nextSettings.saveDirectory,
        downloadMode: nextSettings.downloadMode,
        qualityPreference: nextSettings.qualityPreference,
        autoRevealInFinder: nextSettings.autoRevealInFinder,
        maxConcurrentDownloads: nextSettings.maxConcurrentDownloads,
        proxyUrl: nextSettings.proxyUrl,
        speedLimit: nextSettings.speedLimit,
        autoUpdate: nextSettings.autoUpdate,
        theme: nextSettings.theme,
        notifyOnComplete: nextSettings.notifyOnComplete,
        language: nextSettings.language,
        ffmpegAvailable: nextSettings.ffmpegAvailable
      };
      targetDirectory = "";
      platformAuthDrafts = clonePlatformAuthDrafts(nextSettings.platformAuth);
      successMessage = $t("app.settingsSaved");
    } catch (error) {
      bootstrap = await getBootstrapState();
      syncSettings(bootstrap);
      errorMessage = resolveErrorMessage(error);
    } finally {
      settingsSaving = false;
    }
  }

  function handleOpenSettings() {
    if (!bootstrap) {
      return;
    }

    syncSettings(bootstrap);
    settingsOpen = true;
  }

  function upsertTask(task: DownloadTask) {
    const index = tasks.findIndex((item) => item.id === task.id);
    if (index >= 0) {
      tasks = tasks.map((item) => (item.id === task.id ? task : item));
      return;
    }

    tasks = [task, ...tasks];
  }

  function resolvedTargetDirectory() {
    return targetDirectory.trim() || bootstrap?.saveDirectory || "";
  }

  function selectAllProfileItems() {
    douyinSelectedProfileIds = douyinProfilePreview?.items.map((item) => item.assetId) ?? [];
  }

  function clearProfileSelection() {
    douyinSelectedProfileIds = [];
  }

  function selectAllBilibiliProfileItems() {
    bilibiliSelectedProfileIds =
      bilibiliProfilePreview?.items.map((item) => item.assetId) ?? [];
  }

  function clearBilibiliProfileSelection() {
    bilibiliSelectedProfileIds = [];
  }

  function closeProfileSelection() {
    douyinProfilePreview = null;
    douyinSelectedProfileIds = [];
    douyinSelectedProfileFormatIds = {};
    douyinProfileBrowserSession = null;
  }

  function closeBilibiliProfileSelection() {
    bilibiliProfilePreview = null;
    bilibiliSelectedProfileIds = [];
    bilibiliSelectedProfileFormatIds = {};
  }

  function setDouyinProfileFormat(assetId: string, formatId: string) {
    douyinSelectedProfileFormatIds = {
      ...douyinSelectedProfileFormatIds,
      [assetId]: formatId
    };
  }

  function setBilibiliProfileFormat(assetId: string, formatId: string) {
    bilibiliSelectedProfileFormatIds = {
      ...bilibiliSelectedProfileFormatIds,
      [assetId]: formatId
    };
  }

  function currentQualityLabel() {
    return tRaw("quality." + qualityPreference) || "推荐优先";
  }

  function activeModuleTitle() {
    return activeModule ? tRaw("module." + activeModule + ".label") : "StreamVerse";
  }

  async function windowMinimize() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().minimize();
  }
  async function windowToggleMaximize() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().toggleMaximize();
  }
  async function windowClose() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    getCurrentWindow().close();
  }
</script>

{#if loading}
  <main class="loading-shell">
    <div class="drag-region"></div>
    <div class="window-controls">
      <button class="win-btn" onclick={windowMinimize} title="最小化">&#x2013;</button>
      <button class="win-btn" onclick={windowToggleMaximize} title="最大化">&#x25A1;</button>
      <button class="win-btn win-close" onclick={windowClose} title="关闭">&#x2715;</button>
    </div>
    <div class="pulse-card">
      <span class="pulse-dot"></span>
      {$t("app.loading")}
    </div>
  </main>
{:else if bootstrap}
  <main class="app-shell">
    <div class="drag-region"></div>
    <div class="window-controls">
      <button class="win-btn" onclick={windowMinimize} title="最小化">&#x2013;</button>
      <button class="win-btn" onclick={windowToggleMaximize} title="最大化">&#x25A1;</button>
      <button class="win-btn win-close" onclick={windowClose} title="关闭">&#x2715;</button>
    </div>
    <section class="workspace">
      <header class="topbar">
        <div class="brand">
          <p class="eyebrow">StreamVerse</p>
          <h1>{activeModuleTitle()}</h1>
          <p class="status-copy">
            {$t("auth." + bootstrap.authState)}
          </p>
        </div>

        <div class="topbar-actions">
          {#if activeModule}
            <button class="ghost-button" onclick={backToPlatformHome}>{$t("app.backToHome")}</button>
          {/if}
          <button class="ghost-button" onclick={handleOpenSettings}>{$t("app.settings")}</button>
        </div>
      </header>

      {#if activeModule}
        <div class="workspace-switch">
          {#each moduleOrder.filter((moduleId) => moduleEnabled(moduleId)) as moduleId}
            {@const meta = moduleCatalog[moduleId]}
            <button
              class:active={activeModule === moduleId}
              class="workspace-tab"
              onclick={() => openModule(moduleId)}
              type="button"
            >
              <strong>{$t("module." + moduleId + ".label")}</strong>
              <span>{meta.badge}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if activeModule}
        <SharedDirectoryBar
          currentDirectory={resolvedTargetDirectory()}
          defaultDirectory={bootstrap.saveDirectory}
          disabled={
            pickingTargetDirectory ||
            enqueuingDouyinProfile ||
            enqueuingBilibiliProfile ||
            downloadingDouyinSingle ||
            downloadingBilibili ||
            downloadingYoutube ||
            openingFolder
          }
          picking={pickingTargetDirectory}
          on:open={handleOpenCurrentDirectory}
          on:pick={handlePickTargetDirectory}
          on:reset={() => (targetDirectory = bootstrap!.saveDirectory)}
        />
      {/if}

      {#if errorMessage}
        <div class="notice error notice-stack">
          <span>{errorMessage}</span>
        </div>
      {/if}

      {#if successMessage}
        <p class="notice success">{successMessage}</p>
      {/if}

      {#if !activeModule}
        <PlatformHome
          modules={moduleStates}
          on:open={(event) => openModule(event.detail.moduleId)}
        />
      {:else if activeModule === "douyin-single"}
        <section class="platform-workspace">
          <SingleVideoWorkspace
            authState={authStateFor("douyin")}
            bind:downloadOptions={douyinSingleOptions}
            bind:inputValue={douyinSingleInput}
            bind:selectedFormatId={douyinSelectedFormatId}
            description={isWindowsPlatform
              ? "新手可直接粘贴作品链接后点“开始解析”。如果提示需要登录，请到设置里导入一次浏览器 Cookie 或粘贴 Cookie 文本，后续会自动复用。"
              : "粘贴作品链接后开始解析，再选择清晰度下载。"}
            downloading={downloadingDouyinSingle}
            formatNote="抖音默认优先推荐兼容性更高的视频格式；如果本地播放器异常，可切换列表中的其他清晰度重试。"
            heading={$t("douyin.heading")}
            parserLabel=""
            pasting={pastingDouyinSingle}
            platformLabel={$t("module.douyin-single.label")}
            placeholder={$t("douyin.placeholder")}
            preview={douyinSinglePreview}
            qualityLabel={currentQualityLabel()}
            qualityPreference={qualityPreference}
            analyzing={analyzingDouyinSingle}
            analysisProgress={douyinSingleAnalysisProgress}
            on:analyze={handleAnalyzeDouyinSingle}
            on:download={() =>
              douyinSinglePreview &&
              startSingleDownload(
                "douyin",
                douyinSinglePreview,
                douyinSelectedFormatId,
                douyinSingleOptions,
                false
              )}
            on:paste={() => pasteAndAnalyze("douyin", "single")}
          />
        </section>
      {:else if activeModule === "douyin-profile"}
        <section class="platform-workspace">
          <ProfileBatchWorkspace
            bind:downloadOptions={douyinProfileOptions}
            bind:inputValue={douyinProfileInput}
            authState={authStateFor("douyin")}
            description={cookieFileFor("douyin")
              ? "已检测到已保存登录态，直接点“读取主页”即可。若结果异常，再到设置里重新导入一次 Cookie。"
              : "首次使用请先到设置导入一次浏览器 Cookie 或粘贴 Cookie 文本，然后返回这里读取主页作品列表。"}
            downloadedAssetIds={douyinDownloadedAssetIds}
            heading={$t("douyin.profileHeading")}
            heroEyebrow="Profile Batch"
            itemLabel={$t("douyin.itemLabel")}
            preparing={openingDouyinProfileBrowser}
            prepareLabel={$t("douyin.openBrowser")}
            prepareLoadingLabel={$t("douyin.openingBrowser")}
            placeholder={$t("douyin.profilePlaceholder")}
            analyzing={analyzingDouyinProfile}
            analysisProgress={douyinProfileAnalysisProgress}
            analyzeDisabled={!cookieFileFor("douyin") && !douyinProfileBrowserSession}
            enqueuing={enqueuingDouyinProfile}
            pasting={pastingDouyinProfile}
            preview={douyinProfilePreview}
            selectedIds={douyinSelectedProfileIds}
            selectedFormatIdsByAssetId={douyinSelectedProfileFormatIds}
            showPrepareAction={true}
            analyzeLabel={$t("douyin.analyzeLabel")}
            analyzeLoadingLabel={$t("douyin.analyzeLoading")}
            on:prepare={handleOpenDouyinProfileBrowser}
            on:analyze={handleAnalyzeDouyinProfile}
            on:clearSelection={clearProfileSelection}
            on:close={closeProfileSelection}
            on:enqueue={handleEnqueueDouyinProfileTasks}
            on:formatChange={(event) =>
              setDouyinProfileFormat(event.detail.assetId, event.detail.formatId)}
            on:paste={handlePasteDouyinProfileInput}
            on:selectionChange={(event) => (douyinSelectedProfileIds = event.detail.ids)}
            on:selectAll={selectAllProfileItems}
          />
        </section>
      {:else if activeModule === "bilibili-single"}
        <section class="platform-workspace">
          <SingleVideoWorkspace
            authState={authStateFor("bilibili")}
            bind:downloadOptions={bilibiliOptions}
            bind:inputValue={bilibiliInput}
            bind:selectedFormatId={bilibiliSelectedFormatId}
            description={isWindowsPlatform
              ? "直接粘贴哔哩哔哩视频链接即可解析。若遇到会员或登录限制内容，请到设置里导入一次 Cookie，之后可持续使用。"
              : "粘贴视频链接后开始解析，再选择清晰度下载。"}
            downloading={downloadingBilibili}
            formatNote=""
            heading={$t("bilibili.heading")}
            parserLabel=""
            pasting={pastingBilibili}
            platformLabel="Bilibili"
            placeholder={$t("bilibili.placeholder")}
            preview={bilibiliPreview}
            qualityLabel={currentQualityLabel()}
            qualityPreference={qualityPreference}
            analyzing={analyzingBilibili}
            analysisProgress={bilibiliAnalysisProgress}
            on:analyze={handleAnalyzeBilibiliSingle}
            on:download={() =>
              bilibiliPreview &&
              startSingleDownload(
                "bilibili",
                bilibiliPreview,
                bilibiliSelectedFormatId,
                bilibiliOptions,
                false
              )}
            on:paste={() => pasteAndAnalyze("bilibili", "single")}
          />
        </section>
      {:else if activeModule === "bilibili-profile"}
        <section class="platform-workspace">
          <ProfileBatchWorkspace
            bind:downloadOptions={bilibiliProfileOptions}
            bind:inputValue={bilibiliProfileInput}
            authState={authStateFor("bilibili")}
            analyzing={analyzingBilibiliProfile}
            analysisProgress={bilibiliProfileAnalysisProgress}
            analyzeLabel={$t("bilibili.analyzeLabel")}
            analyzeLoadingLabel={$t("bilibili.analyzeLoading")}
            description={cookieFileFor("bilibili")
              ? "已保存登录态，直接读取主页即可；若报错，可在设置中重新导入一次 Cookie。"
              : "若主页读取失败，请先到设置导入一次哔哩哔哩 Cookie，再回来重试。"}
            downloadedAssetIds={bilibiliDownloadedAssetIds}
            enqueuing={enqueuingBilibiliProfile}
            enqueueLabel={$t("bilibili.enqueueLabel")}
            enqueuingLabel={$t("bilibili.enqueuingLabel")}
            heading={$t("bilibili.profileHeading")}
            heroEyebrow="Creator Batch"
            itemLabel={$t("bilibili.itemLabel")}
            pasting={pastingBilibiliProfile}
            placeholder={$t("bilibili.profilePlaceholder")}
            preview={bilibiliProfilePreview}
            selectedIds={bilibiliSelectedProfileIds}
            selectedFormatIdsByAssetId={bilibiliSelectedProfileFormatIds}
            resultEyebrow="Creator Result"
            on:analyze={handleAnalyzeBilibiliProfile}
            on:clearSelection={clearBilibiliProfileSelection}
            on:close={closeBilibiliProfileSelection}
            on:enqueue={handleEnqueueBilibiliProfileTasks}
            on:formatChange={(event) =>
              setBilibiliProfileFormat(event.detail.assetId, event.detail.formatId)}
            on:paste={() => pasteAndAnalyze("bilibili", "profile")}
            on:selectionChange={(event) => (bilibiliSelectedProfileIds = event.detail.ids)}
            on:selectAll={selectAllBilibiliProfileItems}
          />
        </section>
      {:else}
        <section class="platform-workspace">
          <SingleVideoWorkspace
            authState={authStateFor("youtube")}
            bind:downloadOptions={youtubeOptions}
            bind:inputValue={youtubeInput}
            bind:selectedFormatId={youtubeSelectedFormatId}
            description=""
            downloading={downloadingYoutube}
            formatNote=""
            heading={$t("youtube.heading")}
            parserLabel=""
            pasting={pastingYoutube}
            platformLabel="YouTube"
            placeholder={$t("youtube.placeholder")}
            preview={youtubePreview}
            qualityLabel={currentQualityLabel()}
            qualityPreference={qualityPreference}
            analyzing={analyzingYoutube}
            analysisProgress={youtubeAnalysisProgress}
            on:analyze={handleAnalyzeYoutubeSingle}
            on:download={() =>
              youtubePreview &&
              startSingleDownload(
                "youtube",
                youtubePreview,
                youtubeSelectedFormatId,
                youtubeOptions,
                false
              )}
            on:paste={() => pasteAndAnalyze("youtube", "single")}
          />
        </section>
      {/if}

      <TaskQueuePanel
        tasks={tasks}
        pendingTaskIds={taskActionPendingIds}
        {clearingFinished}
        on:cancel={(event) => handleTaskControl(event.detail.task, "cancel")}
        on:clearFinished={handleClearFinished}
        on:pause={(event) => handleTaskControl(event.detail.task, "pause")}
        on:resume={(event) => handleTaskControl(event.detail.task, "resume")}
        on:retry={(event) => handleTaskControl(event.detail.task, "retry")}
        on:reveal={(event) => handleRevealTask(event.detail.task)}
      />

      <SettingsPanel
        open={settingsOpen}
        bind:autoRevealInFinder
        bind:platformAuthDrafts
        bind:qualityPreference
        bind:saveDirectoryDraft
        bind:maxConcurrentDownloads
        bind:proxyUrl
        bind:speedLimit
        bind:autoUpdate
        bind:theme
        bind:notifyOnComplete
        bind:language
        accountLabel={bootstrap.accountLabel}
        browserOptions={browserOptions}
        ffmpegAvailable={bootstrap.ffmpegAvailable}
        isWindows={isWindowsPlatform}
        pickingDirectory={pickingDirectory}
        pickingCookieFilePlatform={pickingCookieFilePlatform}
        platformAuthProfiles={bootstrap.platformAuth}
        qualityOptions={qualityOptions}
        settingsSaving={settingsSaving}
        on:close={() => { if (bootstrap) syncSettings(bootstrap); settingsOpen = false; }}
        on:pickCookieFile={(event) => handlePickCookieFile(event.detail.platform)}
        on:pickDirectory={handlePickSaveDirectory}
        on:save={handleSaveSettings}
      />
    </section>

    {#if analysisModalOpen}
      <div class="analysis-modal-overlay">
        <div class="analysis-modal" class:done={analysisModalDone}>
          {#if analysisModalDone}
            <div class="analysis-done-icon">
              <svg viewBox="0 0 52 52" class="checkmark-svg">
                <circle class="checkmark-circle" cx="26" cy="26" r="23" fill="none"/>
                <path class="checkmark-path" fill="none" d="M15 27l7 7 15-15"/>
              </svg>
            </div>
            <p class="analysis-modal-label done-label">解析完成</p>
            <p class="analysis-modal-message">正在整理数据…</p>
          {:else if analysisModalProgress}
            {@const percent = Math.max(0, Math.min(100, Math.round((analysisModalProgress.current / Math.max(analysisModalProgress.total, 1)) * 100)))}
            <p class="analysis-modal-label">{analysisModalLabel}</p>
            <div class="analysis-modal-stats">
              <span class="analysis-modal-counter">{analysisModalProgress.total > 0 ? `${analysisModalProgress.current} / ${analysisModalProgress.total}` : `已解析 ${analysisModalProgress.current}，统计总数中`}</span>
              <span class="analysis-modal-percent">{percent}%</span>
            </div>
            <div class="task-progress analysis-modal-bar">
              <div
                class="task-progress-fill"
                class:indeterminate={percent < 3}
                style="width: {Math.max(3, percent)}%"
              ></div>
            </div>
            <p class="analysis-modal-message">{analysisModalProgress.message}</p>
          {:else}
            <p class="analysis-modal-label">{analysisModalLabel}</p>
            <div class="task-progress analysis-modal-bar">
              <div class="task-progress-fill indeterminate" style="width: 100%"></div>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </main>
{/if}
