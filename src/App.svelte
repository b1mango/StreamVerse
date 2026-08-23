<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    CheckCircle2,
    CircleAlert,
    ClipboardPaste,
    Download,
    FolderOpen,
    History,
    Link2,
    ListVideo,
    LoaderCircle,
    Menu,
    PanelRightClose,
    PanelRightOpen,
    Search,
    Settings,
    SquareStack,
    Trash2,
    X
  } from "@lucide/svelte";
  import appIconUrl from "../src-tauri/icons/icon.png";
  import AnalysisProgress from "./lib/components/AnalysisProgress.svelte";
  import BatchList from "./lib/components/BatchList.svelte";
  import ContentOptions from "./lib/components/ContentOptions.svelte";
  import PlatformIcon from "./lib/components/PlatformIcon.svelte";
  import SettingsSheet from "./lib/components/SettingsSheet.svelte";
  import SingleFormatList from "./lib/components/SingleFormatList.svelte";
  import TaskPanel from "./lib/components/TaskPanel.svelte";
  import Thumb from "./lib/components/Thumb.svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import {
    analyzeBatchItem,
    analyzeInput,
    analyzeProfileInput,
    clearAnalysisProgress,
    clearFinishedTasks,
    clearPlatformAuth,
    controlTask,
    createDownloadTask,
    createProfileDownloadTasks,
    fetchThumbnail,
    getAnalysisProgress,
    getBootstrapState,
    importBrowserCookies,
    isFramelessWindows,
    listBrowserSources,
    listDownloadHistory,
    openInFileManager,
    pickCookieFile,
    pickSaveDirectory,
    removeDownloadTask,
    saveManualCookies,
    saveSettings,
    subscribeTaskEvents
  } from "./lib/backend";
  import { setLanguage, t } from "./lib/i18n";
  import { createDefaultDownloadOptions, formatDuration, hasSelectedDownloadOptions, resolveErrorMessage, visibleFormats } from "./lib/media";
  import { validateInputTarget, type WorkflowMode } from "./lib/input-validation";
  import { platformMeta } from "./lib/options";
  import type {
    AnalysisProgress as AnalysisProgressState,
    BootstrapState,
    BrowserSource,
    CookieImportResult,
    DownloadHistoryEntry,
    DownloadTask,
    PlatformId,
    ProfileBatch,
    SaveSettingsPayload,
    TaskEvent,
    VideoAsset
  } from "./lib/types";

  type View = "download" | "history";
  let bootstrap = $state<BootstrapState | null>(null);
  let view = $state<View>("download");
  let platform = $state<PlatformId>("douyin");
  let windowMaximized = $state(false);
  let workflowMode = $state<WorkflowMode>("single");
  let rawInput = $state("");
  let analyzing = $state(false);
  let analysisProgress = $state<AnalysisProgressState | null>(null);
  let operationBusy = $state(false);
  let toasts = $state<{ id: number; kind: "success" | "error"; text: string }[]>([]);
  let errorMessage = $state("");
  let preview = $state<VideoAsset | null>(null);
  let previewCoverUrl = $state<string | null>(null);
  let previewCoverFailed = $state(false);
  let profile = $state<ProfileBatch | null>(null);
  let selectedFormatId = $state("");
  let formatsExpanded = $state(false);
  let selectedProfileIds = $state<Set<string>>(new Set());
  let selectedProfileFormats = $state<Record<string, string>>({});
  let lastSelectionIndex = $state<number | null>(null);
  let downloadOptions = $state(createDefaultDownloadOptions());
  let settingsOpen = $state(false);
  let queueCollapsed = $state(false);
  let browserSources = $state<BrowserSource[]>([]);
  let history = $state<DownloadHistoryEntry[]>([]);
  let historyLoading = $state(false);
  let analysisGeneration = 0;
  // YouTube 清晰度补全用独立的代际与登记表：切换模式不打断，重新解析/切平台才中止
  let hydrationGeneration = 0;
  const youtubeHydrations = new Map<string, Promise<number>>();

  // 每个模式一份解析结果快照：切换 单视频/主页/合集 时不丢弃已解析内容
  type WorkspaceSnapshot = {
    rawInput: string;
    preview: VideoAsset | null;
    previewCoverUrl: string | null;
    previewCoverFailed: boolean;
    profile: ProfileBatch | null;
    selectedFormatId: string;
    formatsExpanded: boolean;
    selectedProfileIds: Set<string>;
    selectedProfileFormats: Record<string, string>;
    lastSelectionIndex: number | null;
  };
  const workspaceSnapshots = new Map<WorkflowMode, WorkspaceSnapshot>();

  let authStatus = $derived(bootstrap?.platformAuth[platform]?.status ?? "guest");
  let previewFormats = $derived(visibleFormats(preview, authStatus));
  let selectedFormat = $derived(previewFormats.find((format) => format.id === selectedFormatId));
  let previewIsAlbum = $derived(Boolean(preview?.imageUrls?.length));
  let canDownloadSingle = $derived(
    Boolean(preview) &&
    hasSelectedDownloadOptions(downloadOptions) &&
    (!downloadOptions.downloadVideo || previewIsAlbum || Boolean(selectedFormatId))
  );
  let selectedCount = $derived(selectedProfileIds.size);
  let activeTaskCount = $derived(bootstrap?.tasks.filter((task) => ["queued", "downloading", "paused"].includes(task.status)).length ?? 0);

  let toastSequence = 0;

  function pushToast(kind: "success" | "error", text: string) {
    const id = ++toastSequence;
    // toast 文案统一去掉结尾句号，保持轻量
    const message = text.replace(/[。.!！]+$/u, "").trim();
    toasts = [...toasts.slice(-2), { id, kind, text: message }];
    window.setTimeout(() => {
      toasts = toasts.filter((toast) => toast.id !== id);
    }, 4600);
  }

  function dismissToast(id: number) {
    toasts = toasts.filter((toast) => toast.id !== id);
  }

  function formatHistoryTime(value: string) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds) || seconds <= 0) return value;
    return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenResize: (() => void) | undefined;
    void (async () => {
      bootstrap = await getBootstrapState();
      document.documentElement.dataset.theme = bootstrap.theme;
      document.documentElement.lang = bootstrap.language;
      setLanguage(bootstrap.language);
      preview = null;
      unlisten = await subscribeTaskEvents(applyTaskEvent);
      if (isFramelessWindows()) {
        // 透明窗口下监听最大化：最大化时容器退化为直角满屏
        const api = await import("@tauri-apps/api/window");
        const win = api.getCurrentWindow();
        windowMaximized = await win.isMaximized();
        unlistenResize = await win.onResized(async () => {
          windowMaximized = await win.isMaximized();
        });
      }
    })().catch((error) => (errorMessage = resolveErrorMessage(error)));
    return () => { unlisten?.(); unlistenResize?.(); };
  });

  function applyTaskEvent(event: TaskEvent) {
    if (!bootstrap) return;
    if (event.type === "reset") {
      bootstrap.tasks = event.tasks;
      return;
    }
    if (event.type === "delete") {
      bootstrap.tasks = bootstrap.tasks.filter((task) => task.id !== event.taskId);
      return;
    }
    const existing = bootstrap.tasks.findIndex((task) => task.id === event.task.id);
    bootstrap.tasks = existing === -1
      ? [event.task, ...bootstrap.tasks]
      : bootstrap.tasks.map((task) => (task.id === event.task.id ? event.task : task));
  }

  function selectPlatform(next: PlatformId) {
    if (next === platform) return;
    platform = next;
    if (next !== "youtube" && workflowMode === "playlist") workflowMode = "single";
    workspaceSnapshots.clear();
    resetWorkspace();
  }

  function selectWorkflowMode(next: WorkflowMode) {
    if (next === workflowMode) return;
    workspaceSnapshots.set(workflowMode, {
      rawInput,
      preview,
      previewCoverUrl,
      previewCoverFailed,
      profile,
      selectedFormatId,
      formatsExpanded,
      selectedProfileIds,
      selectedProfileFormats,
      lastSelectionIndex
    });
    workflowMode = next;
    const snapshot = workspaceSnapshots.get(next);
    analysisGeneration += 1;
    analysisProgress = null;
    errorMessage = "";
    if (snapshot) {
      rawInput = snapshot.rawInput;
      preview = snapshot.preview;
      previewCoverUrl = snapshot.previewCoverUrl;
      previewCoverFailed = snapshot.previewCoverFailed;
      profile = snapshot.profile;
      selectedFormatId = snapshot.selectedFormatId;
      formatsExpanded = snapshot.formatsExpanded;
      selectedProfileIds = snapshot.selectedProfileIds;
      selectedProfileFormats = snapshot.selectedProfileFormats;
      lastSelectionIndex = snapshot.lastSelectionIndex;
      // YouTube 清晰度补全在后台持续进行；若会话已结束且有残留待读条目，则补齐
      if (profile && profile.items.some((item) => item.formatStatus === "pending")) {
        const batch = profile;
        void hydrateYouTubeBatchFormats(batch, next).catch(() => 0);
      }
    } else {
      rawInput = "";
      preview = null;
      previewCoverUrl = null;
      previewCoverFailed = false;
      profile = null;
      selectedProfileIds = new Set();
      selectedProfileFormats = {};
      selectedFormatId = "";
      formatsExpanded = false;
      lastSelectionIndex = null;
    }
  }

  function resetWorkspace() {
    analysisGeneration += 1;
    hydrationGeneration += 1;
    youtubeHydrations.clear();
    rawInput = "";
    preview = null;
    previewCoverUrl = null;
    previewCoverFailed = false;
    profile = null;
    selectedProfileIds = new Set();
    selectedProfileFormats = {};
    selectedFormatId = "";
    formatsExpanded = false;
    analysisProgress = null;
    errorMessage = "";
  }

  function startProgressPolling(sessionId: string) {
    let stopped = false;
    let reading = false;
    const read = async () => {
      if (stopped || reading) return;
      reading = true;
      try {
        const next = await getAnalysisProgress(sessionId);
        if (!stopped && next) analysisProgress = next;
      } catch {
        // A progress read is advisory; the parser result remains authoritative.
      } finally {
        reading = false;
      }
    };
    void read();
    const timer = window.setInterval(read, 350);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }

  async function pasteInput() {
    try {
      rawInput = await navigator.clipboard.readText();
    } catch {
      errorMessage = "无法读取剪贴板。";
    }
  }

  async function thumbnailIsBlank(dataUrl: string) {
    const image = new Image();
    image.src = dataUrl;
    try {
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = 32;
      canvas.height = 18;
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (!context) return false;
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let sum = 0;
      let squareSum = 0;
      const count = pixels.length / 4;
      for (let index = 0; index < pixels.length; index += 4) {
        const luminance = (pixels[index] + pixels[index + 1] + pixels[index + 2]) / 3;
        sum += luminance;
        squareSum += luminance * luminance;
      }
      const mean = sum / count;
      const variance = squareSum / count - mean * mean;
      return mean < 4 && variance < 8;
    } catch {
      return false;
    }
  }

  async function loadPreviewCover(asset: VideoAsset) {
    const candidates = Array.from(
      new Set([...(asset.coverUrls ?? []), asset.coverUrl].filter((value): value is string => Boolean(value)))
    );
    if (candidates.length === 0) return;

    let lastError = "没有可用封面。";
    for (const candidate of candidates) {
      try {
        const thumbnail = await fetchThumbnail(candidate);
        if (asset.platform === "douyin" && await thumbnailIsBlank(thumbnail)) {
          lastError = "候选封面是空白图片。";
          continue;
        }
        if (preview?.assetId === asset.assetId && preview.sourceUrl === asset.sourceUrl) {
          preview = { ...preview, coverUrl: candidate };
          previewCoverUrl = thumbnail;
          previewCoverFailed = false;
        }
        return;
      } catch (error) {
        lastError = resolveErrorMessage(error);
      }
    }
    if (preview?.assetId === asset.assetId && preview.sourceUrl === asset.sourceUrl) {
      previewCoverFailed = true;
      pushToast("error", `作品已解析，但缩略图加载失败：${lastError}`);
    }
  }

  function hydrateYouTubeBatchFormats(batch: ProfileBatch, ownerMode: WorkflowMode): Promise<number> {
    // 同一批次只跑一个补全会话：切走时会话在后台继续，切回后直接加入进行中的会话
    const key = (batch.hydrationKey ??= crypto.randomUUID());
    const existing = youtubeHydrations.get(key);
    if (existing) return existing;
    const session = runYouTubeBatchHydration(batch, ownerMode, key).catch(() => 0);
    youtubeHydrations.set(key, session);
    void session.then(() => youtubeHydrations.delete(key));
    return session;
  }

  async function runYouTubeBatchHydration(batch: ProfileBatch, ownerMode: WorkflowMode, key: string): Promise<number> {
    const generation = ++hydrationGeneration;
    // 已加载的条目保持原样（恢复或加入会话时避免重复请求）
    const pendingItems = batch.items.map((item) =>
      item.formatStatus === "loaded" ? item : { ...item, formatStatus: "pending" as const }
    );
    let wrapper: ProfileBatch = { ...batch, items: pendingItems };
    profile = wrapper;

    let cursor = 0;
    let completed = 0;
    let failed = 0;
    let orphaned = false;

    // 进度条与文案均显示真实完成数；条目被拾取时同样触发刷新，
    // 不会卡在（0/N）等待首个 yt-dlp 进程返回
    const reportProgress = () => {
      if (profile?.hydrationKey !== key) return;
      analysisProgress = {
        current: completed,
        total: pendingItems.length,
        message: `正在读取真实清晰度（已完成 ${completed}/${pendingItems.length}）…`
      };
    };
    reportProgress();

    // 把结果提交到用户能看到的地方：仍停留在该模式则更新实时 profile，
    // 否则写入该模式的快照，切回时即可看到最新进度；两处都不再有该批次则中止
    const commit = (index: number, nextItem: VideoAsset) => {
      pendingItems[index] = nextItem;
      const nextWrapper: ProfileBatch = { ...wrapper, items: pendingItems };
      const visibleNow = profile?.hydrationKey === key;
      if (visibleNow) {
        profile = nextWrapper;
      } else {
        const snapshot = workspaceSnapshots.get(ownerMode);
        if (snapshot?.profile?.hydrationKey === key) {
          snapshot.profile = nextWrapper;
        } else {
          orphaned = true;
        }
      }
      wrapper = nextWrapper;
      const formats = visibleFormats(nextItem, "active");
      const selected = formats.find((format) => format.recommended)?.id ?? formats[0]?.id;
      if (selected && !selectedProfileFormats[nextItem.assetId]) {
        selectedProfileFormats = { ...selectedProfileFormats, [nextItem.assetId]: selected };
      }
      reportProgress();
    };

    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

    // 单条目带退避重试：限流/网络抖动等瞬时错误不再直接判死刑
    const resolveItem = async (item: VideoAsset): Promise<VideoAsset> => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        if (generation !== hydrationGeneration || orphaned) break;
        if (attempt > 0) await sleep(900 * attempt + Math.random() * 600);
        try {
          const resolved = await analyzeBatchItem(item.sourceUrl);
          if (resolved.formats.length > 0) {
            return {
              ...item,
              ...resolved,
              categoryLabel: item.categoryLabel,
              groupTitle: item.groupTitle,
              formatStatus: "loaded" as const
            };
          }
        } catch {
          // 进入下一次重试
        }
      }
      return { ...item, formatStatus: "failed" as const };
    };

    const worker = async () => {
      while (generation === hydrationGeneration && !orphaned) {
        const index = cursor;
        cursor += 1;
        const item = pendingItems[index];
        if (!item) return;

        const nextItem = await resolveItem(item);
        if (generation !== hydrationGeneration) return;
        completed += 1;
        if (nextItem.formatStatus === "failed") failed += 1;
        commit(index, nextItem);
      }
    };

    const workers = Math.min(8, pendingItems.length);
    // 错开启动工人，削掉瞬时并发峰值，降低触发 YouTube 限流的概率
    await Promise.all(Array.from({ length: workers }, (_, i) => sleep(i * 160).then(worker)));
    if (generation === hydrationGeneration) {
      if (profile?.hydrationKey === key) analysisProgress = null;
      if (failed > 0) {
        pushToast("error", `${batch.items.length - failed} 个视频已读取真实清晰度，${failed} 个读取失败，可重新解析后再试。`);
      }
    }
    return failed;
  }

  async function analyze() {
    if (!rawInput.trim()) return;
    const validationError = validateInputTarget(rawInput, platform, workflowMode);
    if (validationError) {
      errorMessage = validationError;
      return;
    }

    const generation = ++analysisGeneration;
    const sessionId = crypto.randomUUID();
    const stopProgressPolling = startProgressPolling(sessionId);
    analyzing = true;
    analysisProgress = {
      current: 0,
      total: 0,
      message: workflowMode === "profile"
        ? "正在建立频道作品索引…"
        : workflowMode === "playlist"
          ? "正在建立合集作品索引…"
          : "正在解析作品链接…"
    };
    errorMessage = "";
    try {
      if (workflowMode === "single") {
        const asset = await analyzeInput(rawInput.trim(), sessionId);
        const assetAuthStatus = bootstrap?.platformAuth[asset.platform]?.status ?? "guest";
        const formats = visibleFormats(asset, assetAuthStatus);
        preview = asset;
        previewCoverUrl = null;
        previewCoverFailed = false;
        platform = asset.platform;
        formatsExpanded = false;
        selectedFormatId = formats.find((format) => format.recommended)?.id ?? formats[0]?.id ?? "";
        void loadPreviewCover(asset);
      } else {
        const batch = await analyzeProfileInput(rawInput.trim(), sessionId);
        if (generation !== analysisGeneration) return;
        selectedProfileIds = new Set();
        if (batch.items[0]?.platform === "youtube") {
          stopProgressPolling();
          // 清晰度补全在后台持续进行：切换模式不中断，analyze 也不等待它完成
          void hydrateYouTubeBatchFormats(batch, workflowMode);
        } else {
          profile = batch;
          selectedProfileFormats = Object.fromEntries(batch.items.map((item) => {
            const formats = visibleFormats(item, "active");
            return [item.assetId, formats.find((format) => format.recommended)?.id ?? formats[0]?.id ?? ""];
          }));
        }
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      stopProgressPolling();
      await clearAnalysisProgress(sessionId).catch(() => undefined);
      // YouTube 批量清晰度补全仍在后台进行时，保留它的进度展示
      const hydrating = profile?.items.some((item) => item.formatStatus === "pending") ?? false;
      if (!hydrating) analysisProgress = null;
      analyzing = false;
    }
  }

  async function downloadSingle() {
    if (!bootstrap || !preview) return;
    operationBusy = true;
    errorMessage = "";
    try {
      const format = selectedFormat;
      const task = await createDownloadTask({
        assetId: preview.assetId,
        platform: preview.platform,
        sourceUrl: preview.sourceUrl,
        title: preview.title,
        author: preview.author,
        publishDate: preview.publishDate,
        caption: preview.caption,
        coverUrl: preview.coverUrl,
        imageUrls: preview.imageUrls,
        formatId: format?.id,
        formatLabel: format?.label,
        saveDirectory: bootstrap.saveDirectory,
        downloadOptions,
        autoRevealInFileManager: bootstrap.autoRevealInFinder,
        directUrl: format?.directUrl,
        referer: format?.referer,
        userAgent: format?.userAgent,
        audioDirectUrl: format?.audioDirectUrl,
        audioReferer: format?.audioReferer,
        audioUserAgent: format?.audioUserAgent
      });
      applyTaskEvent({ type: "upsert", task });
      pushToast("success", $t("single.taskCreated"));
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      operationBusy = false;
    }
  }

  async function downloadBatch() {
    if (!bootstrap || !profile || selectedProfileIds.size === 0) return;
    operationBusy = true;
    errorMessage = "";
    try {
      const result = await createProfileDownloadTasks({
        profileTitle: profile.profileTitle,
        sourceUrl: profile.sourceUrl,
        items: profile.items
          .filter((item) => selectedProfileIds.has(item.assetId))
          .map((asset) => ({ asset, selectedFormatId: selectedProfileFormats[asset.assetId] || null })),
        saveDirectoryOverride: bootstrap.saveDirectory,
        downloadOptions
      });
      pushToast("success", result.message);
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      operationBusy = false;
    }
  }

  function toggleProfileItem(id: string, index: number, shift: boolean) {
    const next = new Set(selectedProfileIds);
    if (shift && lastSelectionIndex !== null && profile) {
      const start = Math.min(lastSelectionIndex, index);
      const end = Math.max(lastSelectionIndex, index);
      const shouldSelect = !next.has(id);
      for (let cursor = start; cursor <= end; cursor += 1) {
        const candidate = profile.items[cursor]?.assetId;
        if (candidate) shouldSelect ? next.add(candidate) : next.delete(candidate);
      }
    } else if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedProfileIds = next;
    lastSelectionIndex = index;
  }

  function selectAllProfileItems() {
    if (!profile) return;
    selectedProfileIds = selectedProfileIds.size === profile.items.length
      ? new Set()
      : new Set(profile.items.map((item) => item.assetId));
  }

  function invertProfileSelection() {
    if (!profile) return;
    selectedProfileIds = new Set(profile.items.filter((item) => !selectedProfileIds.has(item.assetId)).map((item) => item.assetId));
  }

  async function handleTaskControl(task: DownloadTask, action: "pause" | "resume" | "cancel" | "retry") {
    try {
      applyTaskEvent({ type: "upsert", task: await controlTask(task.id, action) });
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    }
  }

  async function openSettings() {
    settingsOpen = true;
    try {
      browserSources = await listBrowserSources();
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    }
  }

  async function handleSaveSettings(payload: SaveSettingsPayload) {
    if (!bootstrap) return;
    operationBusy = true;
    try {
      const saved = await saveSettings(payload);
      bootstrap = { ...bootstrap, ...saved };
      document.documentElement.dataset.theme = bootstrap.theme;
      document.documentElement.lang = bootstrap.language;
      setLanguage(bootstrap.language);
      settingsOpen = false;
    } finally {
      operationBusy = false;
    }
  }

  async function handleImportBrowser(platformId: PlatformId, browserId: string, profileId: string | null, consent: "once" | "always", allowElevation: boolean): Promise<CookieImportResult> {
    operationBusy = true;
    try {
      const result = await importBrowserCookies({ platform: platformId, browserId, profileId, consent, allowElevation });
      if (bootstrap && result.status === "active") {
        bootstrap.platformAuth[platformId] = { mode: "browser", browserId, profileId, status: "active", consentedAt: consent === "always" ? Math.floor(Date.now() / 1000) : null };
      }
      return result;
    } finally {
      operationBusy = false;
    }
  }

  async function loadHistory() {
    view = "history";
    historyLoading = true;
    try { history = await listDownloadHistory(200); }
    finally { historyLoading = false; }
  }
</script>

<svelte:head><meta name="theme-color" content="#08070a" /></svelte:head>

<svelte:boundary onerror={(error) => (errorMessage = resolveErrorMessage(error))}>
  <div class="app-shell" class:has-titlebar={isFramelessWindows()} class:maximized={windowMaximized} data-platform={platform} data-language={bootstrap?.language ?? "zh-CN"}>
    <div class="app-frame">
    <TitleBar />
    <aside class="nav-rail" aria-label={$t("app.mainNavigation")}>
      <button class="brand-mark" type="button" title="StreamVerse" aria-label={`StreamVerse ${$t("workspace.title")}`} onclick={() => (view = "download")}><img class="brand-icon" src={appIconUrl} alt="" /></button>
      <nav>
        <button class:active={view === "download"} class="rail-button" type="button" title={$t("common.download")} aria-label={$t("common.download")} onclick={() => (view = "download")}><Download size={19} /></button>
        <button class:active={view === "history"} class="rail-button" type="button" title={$t("history.title")} aria-label={$t("history.title")} onclick={loadHistory}><History size={19} /></button>
      </nav>
      <button class="rail-button" type="button" title={$t("app.settings")} aria-label={$t("app.settings")} onclick={openSettings}><Settings size={19} /></button>
    </aside>

    <main class="main-stage">
      <header class="stage-header">
        <div class="wordmark"><span>STREAM</span><strong>VERSE</strong><small>{bootstrap?.version ?? ""}</small></div>
        <div class="stage-status" class:online={activeTaskCount > 0}><span></span><b>{activeTaskCount > 0 ? `${activeTaskCount} ${$t("task.activeCount")}` : $t("app.systemReady")}</b></div>
        <button class="queue-toggle icon-button" type="button" title={queueCollapsed ? $t("task.expandQueue") : $t("task.collapseQueue")} aria-label={queueCollapsed ? $t("task.expandQueue") : $t("task.collapseQueue")} onclick={() => (queueCollapsed = !queueCollapsed)}>{#if queueCollapsed}<PanelRightOpen size={18} />{:else}<PanelRightClose size={18} />{/if}{#if (bootstrap?.tasks.length ?? 0) > 0}<em class="queue-badge">{bootstrap!.tasks.length}</em>{/if}</button>
      </header>

      {#if view === "history"}
        <section class="history-workspace">
          <header><h1>{$t("history.title")}</h1><strong>{history.length} {$t("history.entries")}</strong></header>
          {#if historyLoading}<div class="center-loader"><LoaderCircle class="spin" size={22} /></div>{:else}
            <div class="history-list">
              {#each history as entry (entry.platform + entry.assetId)}
                <article>
                  <Thumb url={entry.coverUrl} platform={entry.platform} title={entry.title} />
                  <b style:--platform-color={platformMeta[entry.platform].color}>{platformMeta[entry.platform].label}</b>
                  <strong title={entry.title}>{entry.title}</strong>
                  <time>{formatHistoryTime(entry.downloadedAt)}</time>
                  {#if entry.outputPath}
                    <button class="icon-button" type="button" title={$t("task.revealFile")} aria-label={$t("task.revealFile")} onclick={() => openInFileManager(entry.outputPath!, true)}><FolderOpen size={16} /></button>
                  {/if}
                </article>
              {:else}<div class="empty-state">{$t("history.empty")}</div>{/each}
            </div>
          {/if}
        </section>
      {:else}
        <section class="download-workspace">
          <div class="workspace-heading">
            <div><h1>{$t("workspace.title")}</h1></div>
            <div class="platform-switch" role="tablist" aria-label="平台">
              {#each Object.keys(platformMeta) as id}
                <button class:active={platform === id} style:--platform-color={platformMeta[id as PlatformId].color} type="button" role="tab" aria-selected={platform === id} onclick={() => selectPlatform(id as PlatformId)}><PlatformIcon platform={id as PlatformId} size={14} />{platformMeta[id as PlatformId].label}</button>
              {/each}
            </div>
          </div>

          <div class="input-console">
            <div class="console-head">
              <div class="mode-switch" role="tablist" aria-label={$t("workspace.mode")}><button class:active={workflowMode === "single"} type="button" role="tab" onclick={() => selectWorkflowMode("single")}><Download size={15} />{$t("workspace.single")}</button><button class:active={workflowMode === "profile"} type="button" role="tab" onclick={() => selectWorkflowMode("profile")}><SquareStack size={15} />{platform === "youtube" ? $t("workspace.youtubeChannel") : $t("workspace.profile")}</button>{#if platform === "youtube"}<button class:active={workflowMode === "playlist"} type="button" role="tab" onclick={() => selectWorkflowMode("playlist")}><ListVideo size={15} />{$t("workspace.youtubePlaylist")}</button>{/if}</div>
              <button class="console-savedir" type="button" title={$t("settings.downloadPath")} onclick={openSettings}><FolderOpen size={13} /><span>{bootstrap?.saveDirectory ?? "--"}</span></button>
            </div>
            <div class="signal-input">
              <textarea bind:value={rawInput} rows="2" aria-label={$t("workspace.inputLabel")} placeholder={$t("workspace.placeholder")} onkeydown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") analyze(); }}></textarea>
              <button class="icon-button paste-button" type="button" title={$t("workspace.paste")} aria-label={$t("workspace.paste")} onclick={pasteInput}><ClipboardPaste size={16} /></button>
            </div>
            <div class="console-actions">
              <div class="console-meta"><span class="meta-chip" style:--platform-color={platformMeta[platform].color}>{platformMeta[platform].label}</span><span class="meta-chip auth" class:active-auth={authStatus === "active"}><i></i>{authStatus === "active" ? $t("auth.active") : $t("auth.guest")}</span></div>
              <button class="analyze-button" type="button" disabled={analyzing || !rawInput.trim()} onclick={analyze}>{#if analyzing}<LoaderCircle class="spin" size={16} />{:else}<Search size={16} />{/if}<span>{analyzing ? $t("common.analyzing") : $t("common.analyze")}</span></button>
            </div>
          </div>

          {#if analysisProgress}<AnalysisProgress progress={analysisProgress} />{/if}

          {#if errorMessage}<div class="message-strip error" role="alert">{errorMessage}</div>{/if}

          {#if workflowMode === "single"}
            {#if preview}
              <div class="single-result">
                <figure>{#if previewCoverUrl && !previewCoverFailed}<img src={previewCoverUrl} alt={preview.title} width="960" height="540" decoding="async" onerror={() => (previewCoverFailed = true)} />{:else}<div class="cover-placeholder"><PlatformIcon platform={preview.platform} size={52} /></div>{/if}<figcaption><span>{formatDuration(preview.durationSeconds)}</span></figcaption></figure>
                <div class="result-detail"><span class="eyebrow">{platformMeta[preview.platform].label} / {preview.author}</span><h2>{preview.title}</h2><p>{preview.publishDate || "--"}</p>{#if previewIsAlbum}<div class="album-summary"><strong>{$t("content.album")}</strong><span>{preview.imageUrls?.length ?? 0} {$t("content.images")}</span></div>{:else}<SingleFormatList formats={previewFormats} selectedId={selectedFormatId} expanded={formatsExpanded} durationSeconds={preview.durationSeconds} onSelect={(formatId) => (selectedFormatId = formatId)} onExpandedChange={(expanded) => (formatsExpanded = expanded)} />{/if}<ContentOptions options={downloadOptions} onChange={(next) => (downloadOptions = next)} label={$t("single.downloadContent")} /><button class="download-button" type="button" disabled={operationBusy || !canDownloadSingle} onclick={downloadSingle}>{#if operationBusy}<LoaderCircle class="spin" size={18} />{:else}<Download size={18} />{/if}{$t("workspace.enqueue")}</button></div>
              </div>
            {:else}
              <div class="idle-stage">
                <div class="idle-icon"><Link2 size={22} /></div>
                <strong>{$t("workspace.idleTitle")}</strong>
                <p>{$t("workspace.idleHint")}</p>
              </div>
            {/if}
          {:else}
            <div class="batch-workspace">
              <header>
                <div class="batch-summary"><span class="eyebrow">{profile?.profileTitle ?? $t("batch.awaitingSelection")}</span><strong>{$t("batch.fetched")} {profile?.items.length ?? 0} · {$t("batch.selected")} {selectedCount}</strong></div>
                <div class="batch-actions"><button class="quiet-button" type="button" disabled={!profile} onclick={selectAllProfileItems}><Check size={15} />{$t("common.selectAll")}</button><button class="quiet-button" type="button" disabled={!profile} onclick={invertProfileSelection}><Menu size={15} />{$t("batch.invertSelection")}</button><button class="primary-button" type="button" disabled={!profile || selectedCount === 0 || operationBusy || !hasSelectedDownloadOptions(downloadOptions)} onclick={downloadBatch}><Download size={16} />{$t("batch.enqueue")}{selectedCount > 0 ? ` · ${selectedCount}` : ""}</button></div>
              </header>
              {#if profile}<ContentOptions options={downloadOptions} onChange={(next) => (downloadOptions = next)} label={$t("batch.downloadContent")} />{/if}
              <BatchList items={profile?.items ?? []} selectedIds={selectedProfileIds} selectedFormats={selectedProfileFormats} onToggle={toggleProfileItem} onFormat={(id, formatId) => (selectedProfileFormats = { ...selectedProfileFormats, [id]: formatId })} />
            </div>
          {/if}
        </section>
      {/if}
    </main>

    <TaskPanel tasks={bootstrap?.tasks ?? []} collapsed={queueCollapsed} onControl={handleTaskControl} onReveal={(task) => task.outputPath && openInFileManager(task.outputPath, true)} onRemove={async (task) => { await removeDownloadTask(task.id); applyTaskEvent({ type: "delete", taskId: task.id }); }} onClear={async () => { if (!bootstrap) return; bootstrap.tasks = await clearFinishedTasks(); }} />
    </div>

    {#if bootstrap}
      <SettingsSheet open={settingsOpen} {bootstrap} {browserSources} busy={operationBusy} onClose={() => (settingsOpen = false)} onSave={handleSaveSettings} onPickDirectory={() => pickSaveDirectory(bootstrap!.saveDirectory)} onPickCookieFile={pickCookieFile} onImportBrowser={handleImportBrowser} onSaveManual={async (platformId, value) => { const result = await saveManualCookies(platformId, { cookieText: value }); bootstrap!.platformAuth[platformId] = { mode: "manual", status: "active", consentedAt: Math.floor(Date.now() / 1000) }; return result; }} onImportCookieFile={async (platformId, path) => { const result = await saveManualCookies(platformId, { cookieFile: path }); bootstrap!.platformAuth[platformId] = { mode: "manual", status: "active", consentedAt: Math.floor(Date.now() / 1000) }; return result; }} onClearAuth={async (platformId) => { await clearPlatformAuth(platformId); bootstrap!.platformAuth[platformId] = { mode: "none", status: "guest" }; }} />
    {/if}

    <div class="toast-stack" aria-live="polite">
      {#each toasts as toast (toast.id)}
        <div class="toast {toast.kind}" role="status">
          {#if toast.kind === "success"}<CheckCircle2 size={16} />{:else}<CircleAlert size={16} />{/if}
          <span>{toast.text}</span>
          <button class="toast-dismiss" type="button" aria-label={$t("common.close")} onclick={() => dismissToast(toast.id)}><X size={14} /></button>
        </div>
      {/each}
    </div>
  </div>

  {#snippet failed(error, reset)}
    <div class="fatal-boundary"><strong>界面渲染失败</strong><p>{resolveErrorMessage(error)}</p><button class="primary-button" type="button" onclick={reset}>重新加载</button></div>
  {/snippet}
</svelte:boundary>
