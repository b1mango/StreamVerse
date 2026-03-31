<script lang="ts">
  import { onMount } from "svelte";
  import PlatformHome from "./lib/components/PlatformHome.svelte";
  import ProfileBatchWorkspace from "./lib/components/ProfileBatchWorkspace.svelte";
  import SettingsPanel from "./lib/components/SettingsPanel.svelte";
  import SharedDirectoryBar from "./lib/components/SharedDirectoryBar.svelte";
  import SingleVideoWorkspace from "./lib/components/SingleVideoWorkspace.svelte";
  import TaskQueuePanel from "./lib/components/TaskQueuePanel.svelte";
  import {
    analyzeInput,
    analyzeProfileInput,
    cancelDownloadTask,
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
    ModuleId,
    ModuleRuntimeState,
    PlatformId,
    ProfileBatch,
    BrowserLaunchResult,
    QualityPreference,
    SettingsProfile,
    VideoAsset,
    VideoFormat
  } from "./lib/types";

  let loading = true;
  let settingsOpen = false;
  let settingsSaving = false;
  let pickingDirectory = false;
  let pickingTargetDirectory = false;
  let openingFolder = false;
  let clearingFinished = false;
  let activeModule: ModuleId | null = null;
  let bootstrap: BootstrapState | null = null;
  let moduleStates: ModuleRuntimeState[] = [];
  let tasks: DownloadTask[] = [];
  let pollTimer: number | undefined;
  let pollingTasks = false;

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
  let cookieBrowser = "";
  let saveDirectoryDraft = "";
  let targetDirectory = "";
  let downloadMode: DownloadMode = "manual";
  let qualityPreference: QualityPreference = "recommended";
  let autoRevealInFinder = false;
  let taskActionPendingIds: string[] = [];

  const isDesktopRuntime =
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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

    if (!isDesktopRuntime) {
      douyinSinglePreview = bootstrap.preview;
      douyinSingleInput = bootstrap.preview.sourceUrl;
      douyinSelectedFormatId =
        pickPreferredFormat(
          bootstrap.preview,
          bootstrap.qualityPreference,
          bootstrap.authState
        )?.id ?? "";
    }

    if (isDesktopRuntime) {
      pollTimer = window.setInterval(async () => {
        if (pollingTasks) {
          return;
        }

        pollingTasks = true;
        try {
          tasks = await listDownloadTasks();
        } finally {
          pollingTasks = false;
        }
      }, 280);
    }

    loading = false;
  }

  function syncSettings(next: BootstrapState | SettingsProfile) {
    cookieBrowser = next.cookieBrowser ?? "";
    saveDirectoryDraft = next.saveDirectory;
    targetDirectory = next.saveDirectory;
    downloadMode = next.downloadMode;
    qualityPreference = next.qualityPreference;
    autoRevealInFinder = next.autoRevealInFinder;
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
        pickPreferredFormat(item, qualityPreference, bootstrap!.authState)?.id ?? ""
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
    const total = Math.max(current, Math.round(progress.total || 0), 1);
    return {
      current,
      total,
      message: progress.message?.trim() || "正在解析…"
    };
  }

  async function withAnalysisProgress<T>(
    setProgress: (progress: AnalysisProgress | null) => void,
    runner: (sessionId: string) => Promise<T>
  ): Promise<T> {
    const sessionId = createAnalysisSessionId();
    let pollId: number | undefined;
    setProgress({ current: 0, total: 1, message: "准备解析…" });

    if (isDesktopRuntime) {
      pollId = window.setInterval(async () => {
        try {
          const next = await getAnalysisProgress(sessionId);
          if (next) {
            setProgress(normalizeAnalysisProgress(next));
          }
        } catch {}
      }, 180);
    }

    try {
      return await runner(sessionId);
    } finally {
      if (pollId) {
        window.clearInterval(pollId);
      }
      if (isDesktopRuntime) {
        try {
          const finalProgress = await getAnalysisProgress(sessionId);
          if (finalProgress) {
            setProgress(normalizeAnalysisProgress(finalProgress));
          }
        } catch {}
        try {
          await clearAnalysisProgress(sessionId);
        } catch {}
      }
    }
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
        pickPreferredFormat(preview, qualityPreference, bootstrap!.authState)?.id ?? "";

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
        pickPreferredFormat(preview, qualityPreference, bootstrap!.authState)?.id ?? "";

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
        pickPreferredFormat(preview, qualityPreference, bootstrap!.authState)?.id ?? "";

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
      if (!douyinProfileBrowserSession) {
        throw new Error("请先打开浏览器窗口，登录后再点“读取完整列表”。");
      }
      const port = douyinProfileBrowserSession.port;
      douyinProfilePreview = null;
      douyinSelectedProfileIds = [];
      douyinSelectedProfileFormatIds = {};

      const result = await withAnalysisProgress(
        (progress) => (douyinProfileAnalysisProgress = progress),
        (sessionId) =>
          collectProfileBrowser({
            rawInput: douyinProfileInput,
            port,
            sessionId
          })
      );
      douyinProfilePreview = result;
      douyinSelectedProfileIds = result.items.map((item) => item.assetId);
      douyinSelectedProfileFormatIds = buildBatchFormatSelections(result.items);
      successMessage = `已读取 ${result.fetchedCount} 个作品。`;
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
      const result = await withAnalysisProgress(
        (progress) => (bilibiliProfileAnalysisProgress = progress),
        (sessionId) =>
          analyzeProfileInput({
            rawInput: bilibiliProfileInput,
            sessionId
          })
      );

      if (result.items.some((item) => item.platform !== "bilibili")) {
        throw new Error("请使用 Bilibili UP 主空间页链接。");
      }

      bilibiliProfilePreview = result;
      bilibiliSelectedProfileIds = result.items.map((item) => item.assetId);
      bilibiliSelectedProfileFormatIds = buildBatchFormatSelections(result.items);
      successMessage = `已读取 ${result.fetchedCount} 个视频。`;
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
        ? selectedFormat(asset, selectedFormatId, bootstrap!.authState)
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

    settingsSaving = true;
    clearNotices();

    try {
      const nextSettings = await saveSettings({
        cookieBrowser: cookieBrowser || null,
        saveDirectory: saveDirectoryDraft,
        downloadMode,
        qualityPreference,
        autoRevealInFinder
      });

      bootstrap = {
        ...bootstrap,
        authState: nextSettings.authState,
        accountLabel: nextSettings.accountLabel,
        cookieBrowser: nextSettings.cookieBrowser,
        saveDirectory: nextSettings.saveDirectory,
        downloadMode: nextSettings.downloadMode,
        qualityPreference: nextSettings.qualityPreference,
        autoRevealInFinder: nextSettings.autoRevealInFinder,
        ffmpegAvailable: nextSettings.ffmpegAvailable
      };
      syncSettings(nextSettings);
      settingsOpen = false;
      successMessage = "设置已保存。";
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
    return (
      qualityOptions.find((item) => item.value === qualityPreference)?.label ?? "推荐优先"
    );
  }

  function activeModuleTitle() {
    return activeModule ? moduleCatalog[activeModule].label : "StreamVerse";
  }
</script>

{#if loading}
  <main class="loading-shell">
    <div class="pulse-card">
      <span class="pulse-dot"></span>
      正在建立下载工作台…
    </div>
  </main>
{:else if bootstrap}
  <main class="app-shell">
    <section class="workspace">
      <header class="topbar">
        <div class="brand">
          <p class="eyebrow">StreamVerse</p>
          <h1>{activeModuleTitle()}</h1>
          <p class="status-copy">
            {authMap[bootstrap.authState]}
          </p>
        </div>

        <div class="topbar-actions">
          {#if activeModule}
            <button class="ghost-button" onclick={backToPlatformHome}>返回平台首页</button>
          {/if}
          <button class="ghost-button" onclick={handleOpenSettings}>设置</button>
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
              <strong>{meta.label}</strong>
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
            authState={bootstrap.authState}
            bind:downloadOptions={douyinSingleOptions}
            bind:inputValue={douyinSingleInput}
            bind:selectedFormatId={douyinSelectedFormatId}
            description=""
            downloading={downloadingDouyinSingle}
            formatNote=""
            heading="抖音单视频下载"
            parserLabel=""
            pasting={pastingDouyinSingle}
            platformLabel="抖音"
            placeholder="粘贴抖音分享文案、短链或作品页链接"
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
            authState={bootstrap.authState}
            analyzeDisabled={!douyinProfileBrowserSession}
            description=""
            heading="抖音主页批量下载"
            heroEyebrow="Profile Batch"
            itemLabel="作品"
            pasteLabel="粘贴链接"
            pasteLoadingLabel="读取剪贴板…"
            preparing={openingDouyinProfileBrowser}
            prepareLabel="打开浏览器"
            prepareLoadingLabel="打开中…"
            placeholder="粘贴抖音个人主页分享文案或主页链接"
            analyzing={analyzingDouyinProfile}
            analysisProgress={douyinProfileAnalysisProgress}
            enqueuing={enqueuingDouyinProfile}
            pasting={pastingDouyinProfile}
            preview={douyinProfilePreview}
            selectedIds={douyinSelectedProfileIds}
            selectedFormatIdsByAssetId={douyinSelectedProfileFormatIds}
            showPrepareAction={true}
            analyzeLabel="读取完整列表"
            analyzeLoadingLabel="读取中…"
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
            authState={bootstrap.authState}
            bind:downloadOptions={bilibiliOptions}
            bind:inputValue={bilibiliInput}
            bind:selectedFormatId={bilibiliSelectedFormatId}
            description=""
            downloading={downloadingBilibili}
            formatNote=""
            heading="Bilibili 单视频下载"
            parserLabel=""
            pasting={pastingBilibili}
            platformLabel="Bilibili"
            placeholder="粘贴 Bilibili 视频链接、分享文案、BV 号或 b23.tv 短链"
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
            authState={bootstrap.authState}
            analyzing={analyzingBilibiliProfile}
            analysisProgress={bilibiliProfileAnalysisProgress}
            analyzeLabel="解析主页"
            analyzeLoadingLabel="读取视频中…"
            description=""
            enqueuing={enqueuingBilibiliProfile}
            enqueueLabel="将所选视频加入队列"
            enqueuingLabel="加入队列中…"
            heading="Bilibili 主页批量下载"
            heroEyebrow="Creator Batch"
            itemLabel="视频"
            pasting={pastingBilibiliProfile}
            placeholder="粘贴 Bilibili UP 主空间页链接或分享文案"
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
            authState={bootstrap.authState}
            bind:downloadOptions={youtubeOptions}
            bind:inputValue={youtubeInput}
            bind:selectedFormatId={youtubeSelectedFormatId}
            description=""
            downloading={downloadingYoutube}
            formatNote=""
            heading="YouTube 单视频下载"
            parserLabel=""
            pasting={pastingYoutube}
            platformLabel="YouTube"
            placeholder="粘贴 YouTube 视频链接、分享文案或 youtu.be 短链"
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
        bind:cookieBrowser
        bind:qualityPreference
        bind:saveDirectoryDraft
        accountLabel={bootstrap.accountLabel}
        browserOptions={browserOptions}
        ffmpegAvailable={bootstrap.ffmpegAvailable}
        pickingDirectory={pickingDirectory}
        qualityOptions={qualityOptions}
        settingsSaving={settingsSaving}
        on:close={() => (settingsOpen = false)}
        on:pickDirectory={handlePickSaveDirectory}
        on:save={handleSaveSettings}
      />
    </section>
  </main>
{/if}
