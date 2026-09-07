<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    CheckCircle2,
    ChevronDown,
    ChevronUp,
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
    RefreshCw,
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
  import { createDefaultDownloadOptions, formatDuration, hasSelectedDownloadOptions, pickPreferredFormat, resolveErrorMessage, visibleFormats } from "./lib/media";
  import { validateInputTarget, type WorkflowMode } from "./lib/input-validation";
  import { platformMeta } from "./lib/options";
  import { createMacShellDrag } from "./lib/mac-shell";
  import { createYouTubeBatchHydration } from "./lib/youtube-hydration";
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
  let workflowMode = $state<WorkflowMode>("single");
  let rawInput = $state("");
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
  let batchFilter = $state("");
  let lastSelectionIndex = $state<number | null>(null);
  let downloadOptions = $state(createDefaultDownloadOptions());
  let settingsOpen = $state(false);
  let queueCollapsed = $state(false);
  let browserSources = $state<BrowserSource[]>([]);
  let history = $state<DownloadHistoryEntry[]>([]);
  let historyLoading = $state(false);
  // 每个「平台 × 模式」一份解析运行时：三平台的主页/合集/单视频可同时解析，
  // 进度与报错互不覆盖，切换平台或模式后再切回也能看到原进度
  type ModeAnalysis = {
    analyzing: boolean;
    progress: AnalysisProgressState | null;
    error: string;
    generation: number;
  };
  const freshModeAnalysis = (): ModeAnalysis => ({ analyzing: false, progress: null, error: "", generation: 0 });
  const WORKFLOW_MODES: WorkflowMode[] = ["single", "profile", "playlist"];
  const scopeOf = (platformId: PlatformId, mode: WorkflowMode) => `${platformId}:${mode}`;
  let modeAnalysis = $state<Record<string, ModeAnalysis>>(
    Object.fromEntries(
      Object.keys(platformMeta).flatMap((platformId) =>
        WORKFLOW_MODES.map((mode) => [`${platformId}:${mode}`, freshModeAnalysis()])
      )
    )
  );
  const scopeAnalysis = (mode: WorkflowMode) => modeAnalysis[scopeOf(platform, mode)];
  const platformBusy = (platformId: string) =>
    WORKFLOW_MODES.some((mode) => modeAnalysis[`${platformId}:${mode}`]?.analyzing);
  // YouTube 清晰度补全按批次 hydrationKey 认领工作区：主页/合集、
  // 不同平台的补全会话互不干扰，靠 hydrationKey 失配自动终止孤儿会话
  const youtubeHydrations = new Map<string, Promise<number>>();

  // 每个「平台 × 模式」一份解析结果快照：切换平台或模式时不丢弃已解析内容
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
  const workspaceSnapshots = new Map<string, WorkspaceSnapshot>();

  let authStatus = $derived(bootstrap?.platformAuth[platform]?.status ?? "guest");
  let qualityPreference = $derived(bootstrap?.qualityPreference ?? "recommended");
  let previewFormats = $derived(visibleFormats(preview, authStatus));
  let selectedFormat = $derived(previewFormats.find((format) => format.id === selectedFormatId));
  let previewIsAlbum = $derived(Boolean(preview?.imageUrls?.length));
  let canDownloadSingle = $derived(
    Boolean(preview) &&
    hasSelectedDownloadOptions(downloadOptions) &&
    (!downloadOptions.downloadVideo || previewIsAlbum || Boolean(selectedFormatId))
  );
  let selectedCount = $derived(selectedProfileIds.size);
  // 清晰度补全失败的条目数与补全进行中状态，用于「重读失败清晰度」按钮
  let failedFormatCount = $derived(profile?.items.filter((item) => item.formatStatus === "failed").length ?? 0);
  let profileHydrating = $derived(profile?.items.some((item) => item.formatStatus === "pending") ?? false);
  // 批量列表按标题/作者过滤；shift 连选也基于过滤后的可见顺序
  let visibleProfileItems = $derived.by(() => {
    const items = profile?.items ?? [];
    const query = batchFilter.trim().toLowerCase();
    if (!query) return items;
    return items.filter((item) =>
      item.title.toLowerCase().includes(query) || item.author.toLowerCase().includes(query)
    );
  });
  // 批量解析有结果后控制台折叠为细条，把垂直空间让给列表；用户可手动展开改链接
  let batchConsoleExpanded = $state(false);
  let consoleCollapsed = $derived(workflowMode !== "single" && !!profile && !batchConsoleExpanded);
  let activeTaskCount = $derived(bootstrap?.tasks.filter((task) => ["queued", "downloading", "paused"].includes(task.status)).length ?? 0);
  let currentAnalysis = $derived(modeAnalysis[scopeOf(platform, workflowMode)]);

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
    void (async () => {
      bootstrap = await getBootstrapState();
      document.documentElement.dataset.theme = bootstrap.theme;
      document.documentElement.lang = bootstrap.language;
      setLanguage(bootstrap.language);
      preview = null;
      unlisten = await subscribeTaskEvents(applyTaskEvent);
    })().catch((error) => (errorMessage = resolveErrorMessage(error)));
    return () => unlisten?.();
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

  function saveCurrentSnapshot() {
    workspaceSnapshots.set(scopeOf(platform, workflowMode), {
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
  }

  function restoreScopeSnapshot(scope: string) {
    const snapshot = workspaceSnapshots.get(scope);
    errorMessage = "";
    batchFilter = "";
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
        void hydrateYouTubeBatchFormats(batch, scope).catch(() => 0);
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

  function selectPlatform(next: PlatformId) {
    if (next === platform) return;
    // 切换平台不再清空任何解析成果：快照当前作用域，恢复目标平台的作用域
    saveCurrentSnapshot();
    platform = next;
    batchConsoleExpanded = false;
    if (next !== "youtube" && workflowMode === "playlist") workflowMode = "single";
    restoreScopeSnapshot(scopeOf(platform, workflowMode));
  }

  function selectWorkflowMode(next: WorkflowMode) {
    if (next === workflowMode) return;
    batchConsoleExpanded = false;
    saveCurrentSnapshot();
    workflowMode = next;
    restoreScopeSnapshot(scopeOf(platform, next));
  }

  function startProgressPolling(scope: string, sessionId: string) {
    let stopped = false;
    let reading = false;
    const read = async () => {
      if (stopped || reading) return;
      reading = true;
      try {
        const next = await getAnalysisProgress(sessionId);
        if (!stopped && next) modeAnalysis[scope].progress = next;
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

  async function loadPreviewCover(asset: VideoAsset, scope: string) {
    const candidates = Array.from(
      new Set([...(asset.coverUrls ?? []), asset.coverUrl].filter((value): value is string => Boolean(value)))
    );
    if (candidates.length === 0) return;

    // 解析在后台完成时，封面写进该作用域的快照，切回即可看到
    const liveMatch = () => scopeOf(platform, workflowMode) === scope && preview?.assetId === asset.assetId && preview.sourceUrl === asset.sourceUrl;
    const matchesSnapshot = () => {
      const snapshot = workspaceSnapshots.get(scope);
      return snapshot?.preview?.assetId === asset.assetId && snapshot.preview.sourceUrl === asset.sourceUrl
        ? snapshot
        : null;
    };

    let lastError = "没有可用封面。";
    for (const candidate of candidates) {
      try {
        const thumbnail = await fetchThumbnail(candidate);
        if (asset.platform === "douyin" && await thumbnailIsBlank(thumbnail)) {
          lastError = "候选封面是空白图片。";
          continue;
        }
        if (liveMatch() && preview) {
          preview = { ...preview, coverUrl: candidate };
          previewCoverUrl = thumbnail;
          previewCoverFailed = false;
        } else {
          const snapshot = matchesSnapshot();
          if (snapshot?.preview) {
            snapshot.preview = { ...snapshot.preview, coverUrl: candidate };
            snapshot.previewCoverUrl = thumbnail;
            snapshot.previewCoverFailed = false;
          }
        }
        return;
      } catch (error) {
        lastError = resolveErrorMessage(error);
      }
    }
    if (liveMatch()) {
      previewCoverFailed = true;
      pushToast("error", `作品已解析，但缩略图加载失败：${lastError}`);
    } else {
      const snapshot = matchesSnapshot();
      if (snapshot) snapshot.previewCoverFailed = true;
    }
  }

  function hydrateYouTubeBatchFormats(batch: ProfileBatch, ownerScope: string): Promise<number> {
    // 同一批次只跑一个补全会话：切走时会话在后台继续，切回后直接加入进行中的会话
    const key = (batch.hydrationKey ??= crypto.randomUUID());
    const existing = youtubeHydrations.get(key);
    if (existing) return existing;
    const session = runYouTubeBatchHydration(batch, ownerScope, key).catch(() => 0);
    youtubeHydrations.set(key, session);
    void session.then(() => youtubeHydrations.delete(key));
    return session;
  }

  const runYouTubeBatchHydration = createYouTubeBatchHydration({
    analyzeBatchItem,
    pickPreferredFormat,
    currentScope: () => scopeOf(platform, workflowMode),
    getProfile: () => profile,
    setProfile: (next) => { profile = next; },
    getSnapshot: (scope) => workspaceSnapshots.get(scope),
    setProgress: (scope, progress) => { modeAnalysis[scope].progress = progress; },
    getSelectedProfileFormats: () => selectedProfileFormats,
    setSelectedProfileFormats: (next) => { selectedProfileFormats = next; },
    qualityPreference: () => qualityPreference,
    pushToast,
    activeSessionCount: () => youtubeHydrations.size
  });

  async function analyze() {
    const mode = workflowMode;
    const scope = scopeOf(platform, mode);
    const input = rawInput.trim();
    if (!input) return;
    const validationError = validateInputTarget(input, platform, mode);
    if (validationError) {
      modeAnalysis[scope].error = validationError;
      return;
    }

    const runtime = modeAnalysis[scope];
    const generation = ++runtime.generation;
    const sessionId = crypto.randomUUID();
    const stopProgressPolling = startProgressPolling(scope, sessionId);
    runtime.analyzing = true;
    runtime.progress = {
      current: 0,
      total: 0,
      message: mode === "profile"
        ? "正在建立频道作品索引…"
        : mode === "playlist"
          ? "正在建立合集作品索引…"
          : "正在解析作品链接…"
    };
    runtime.error = "";
    errorMessage = "";
    // 发起新解析后收起手动展开的控制台，结果出来后自动回到细条形态
    batchConsoleExpanded = false;
    try {
      if (mode === "single") {
        const asset = await analyzeInput(input, sessionId);
        if (generation !== runtime.generation) return;
        const assetAuthStatus = bootstrap?.platformAuth[asset.platform]?.status ?? "guest";
        const formats = visibleFormats(asset, assetAuthStatus);
        const nextFormatId = pickPreferredFormat(asset, qualityPreference, assetAuthStatus)?.id ?? formats[0]?.id ?? "";
        if (scopeOf(platform, workflowMode) === scope) {
          preview = asset;
          previewCoverUrl = null;
          previewCoverFailed = false;
          platform = asset.platform;
          formatsExpanded = false;
          selectedFormatId = nextFormatId;
        } else {
          // 解析在后台完成：写入该作用域的快照，切回即可看到结果
          const snapshot = workspaceSnapshots.get(scope);
          if (snapshot) {
            snapshot.preview = asset;
            snapshot.previewCoverUrl = null;
            snapshot.previewCoverFailed = false;
            snapshot.formatsExpanded = false;
            snapshot.selectedFormatId = nextFormatId;
          }
        }
        void loadPreviewCover(asset, scope);
      } else {
        const batch = await analyzeProfileInput(input, sessionId);
        if (generation !== runtime.generation) return;
        const isYouTube = batch.items[0]?.platform === "youtube";
        if (scopeOf(platform, workflowMode) === scope) {
          selectedProfileIds = new Set();
        } else {
          const snapshot = workspaceSnapshots.get(scope);
          if (snapshot) {
            snapshot.profile = batch;
            snapshot.selectedProfileIds = new Set();
          }
        }
        if (isYouTube) {
          stopProgressPolling();
          // 清晰度补全在后台持续进行：切换平台或模式不中断，analyze 也不等待它完成
          void hydrateYouTubeBatchFormats(batch, scope);
        } else {
          const nextFormats = Object.fromEntries(batch.items.map((item) => {
            const formats = visibleFormats(item, "active");
            return [item.assetId, pickPreferredFormat(item, qualityPreference, "active")?.id ?? formats[0]?.id ?? ""];
          }));
          if (scopeOf(platform, workflowMode) === scope) {
            profile = batch;
            selectedProfileFormats = nextFormats;
          } else {
            const snapshot = workspaceSnapshots.get(scope);
            if (snapshot) snapshot.selectedProfileFormats = nextFormats;
          }
        }
      }
    } catch (error) {
      runtime.error = resolveErrorMessage(error);
    } finally {
      stopProgressPolling();
      await clearAnalysisProgress(sessionId).catch(() => undefined);
      // YouTube 批量清晰度补全仍在后台进行时，保留它的进度展示
      const batch = scopeOf(platform, workflowMode) === scope ? profile : workspaceSnapshots.get(scope)?.profile;
      const hydrating = batch?.items.some((item) => item.formatStatus === "pending") ?? false;
      if (!hydrating) runtime.progress = null;
      runtime.analyzing = false;
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
    if (shift && lastSelectionIndex !== null && visibleProfileItems.length > 0) {
      const start = Math.min(lastSelectionIndex, index);
      const end = Math.max(lastSelectionIndex, index);
      const shouldSelect = !next.has(id);
      for (let cursor = start; cursor <= end; cursor += 1) {
        const candidate = visibleProfileItems[cursor]?.assetId;
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

  // 只补读清晰度失败的条目：补全会话本身会跳过已加载条目，重读少量失败项无需整批重来
  function retryFailedFormats() {
    if (!profile || failedFormatCount === 0) return;
    void hydrateYouTubeBatchFormats(profile, scopeOf(platform, workflowMode)).catch(() => 0);
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

  // macOS 隐藏标题栏（红绿灯悬浮）下，顶栏与侧栏空白区充当窗口拖拽区，双击缩放
  const {
    macDesktop,
    handleMouseDown: handleMacShellMouseDown,
    handleDblClick: handleMacShellDblClick
  } = createMacShellDrag();
</script>

<svelte:head><meta name="theme-color" content="#08070a" /></svelte:head>

<svelte:window onmousedown={handleMacShellMouseDown} ondblclick={handleMacShellDblClick} />

<svelte:boundary onerror={(error) => (errorMessage = resolveErrorMessage(error))}>
  <div class="app-shell" class:has-titlebar={isFramelessWindows()} class:is-macos={macDesktop} data-platform={platform} data-language={bootstrap?.language ?? "zh-CN"}>
    <div class="app-frame">
    {#if macDesktop}<div class="mac-titlebar" aria-hidden="true"></div>{/if}
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
        <section class="download-workspace" class:batch-mode={workflowMode !== "single"} class:console-open={workflowMode !== "single" && !consoleCollapsed}>
          <div class="workspace-heading">
            <div><h1>{$t("workspace.title")}</h1></div>
            <div class="platform-switch" role="tablist" aria-label="平台">
              {#each Object.keys(platformMeta) as id}
                <button class:active={platform === id} style:--platform-color={platformMeta[id as PlatformId].color} type="button" role="tab" aria-selected={platform === id} onclick={() => selectPlatform(id as PlatformId)}><PlatformIcon platform={id as PlatformId} size={14} />{platformMeta[id as PlatformId].label}{#if platformBusy(id)}<i class="mode-busy-dot" aria-hidden="true"></i>{/if}</button>
              {/each}
            </div>
          </div>

          {#if consoleCollapsed}
            <button class="console-collapsed-bar" type="button" onclick={() => (batchConsoleExpanded = true)}>
              <PlatformIcon {platform} size={14} />
              <strong>{profile?.profileTitle ?? ""}</strong>
              <span>{$t("batch.fetched")} {profile?.items.length ?? 0}</span>
              <i class="collapsed-bar-action"><ChevronDown size={14} />{$t("batch.editLink")}</i>
            </button>
          {:else}
          <div class="input-console">
            <div class="console-head">
              <div class="mode-switch" role="tablist" aria-label={$t("workspace.mode")} style:--platform-color={platformMeta[platform].color}><button class:active={workflowMode === "single"} type="button" role="tab" onclick={() => selectWorkflowMode("single")}><Download size={15} />{$t("workspace.single")}{#if scopeAnalysis("single").analyzing}<i class="mode-busy-dot" aria-hidden="true"></i>{/if}</button><button class:active={workflowMode === "profile"} type="button" role="tab" onclick={() => selectWorkflowMode("profile")}><SquareStack size={15} />{platform === "youtube" ? $t("workspace.youtubeChannel") : $t("workspace.profile")}{#if scopeAnalysis("profile").analyzing}<i class="mode-busy-dot" aria-hidden="true"></i>{/if}</button>{#if platform === "youtube"}<button class:active={workflowMode === "playlist"} type="button" role="tab" onclick={() => selectWorkflowMode("playlist")}><ListVideo size={15} />{$t("workspace.youtubePlaylist")}{#if scopeAnalysis("playlist").analyzing}<i class="mode-busy-dot" aria-hidden="true"></i>{/if}</button>{/if}</div>
              {#if workflowMode !== "single" && profile}
                <button class="icon-button console-collapse" type="button" title={$t("batch.collapseConsole")} aria-label={$t("batch.collapseConsole")} onclick={() => (batchConsoleExpanded = false)}><ChevronUp size={15} /></button>
              {/if}
            </div>
            <div class="signal-input">
              <textarea bind:value={rawInput} rows="2" aria-label={$t("workspace.inputLabel")} placeholder={$t("workspace.placeholder")} onkeydown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") analyze(); }}></textarea>
              <button class="icon-button paste-button" type="button" title={$t("workspace.paste")} aria-label={$t("workspace.paste")} onclick={pasteInput}><ClipboardPaste size={16} /></button>
            </div>
            <div class="console-actions">
              <div class="console-meta"><span class="meta-chip" style:--platform-color={platformMeta[platform].color}>{platformMeta[platform].label}</span><span class="meta-chip auth" class:active-auth={authStatus === "active"}><i></i>{authStatus === "active" ? $t("auth.active") : $t("auth.guest")}</span></div>
              <button class="analyze-button" type="button" disabled={currentAnalysis.analyzing || !rawInput.trim()} onclick={analyze}>{#if currentAnalysis.analyzing}<LoaderCircle class="spin" size={16} />{:else}<Search size={16} />{/if}<span>{currentAnalysis.analyzing ? $t("common.analyzing") : $t("common.analyze")}</span></button>
            </div>
          </div>
          {/if}

          {#if currentAnalysis.progress}<AnalysisProgress progress={currentAnalysis.progress} />{/if}

          {#if currentAnalysis.error || errorMessage}<div class="message-strip error" role="alert">{currentAnalysis.error || errorMessage}</div>{/if}

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
                <div class="batch-actions">{#if failedFormatCount > 0}<button class="quiet-button" type="button" disabled={profileHydrating} onclick={retryFailedFormats}><RefreshCw size={14} class={profileHydrating ? "spin" : ""} />{$t("batch.retryFailed")} · {failedFormatCount}</button>{/if}<button class="quiet-button" type="button" disabled={!profile} onclick={selectAllProfileItems}><Check size={15} />{$t("common.selectAll")}</button><button class="quiet-button" type="button" disabled={!profile} onclick={invertProfileSelection}><Menu size={15} />{$t("batch.invertSelection")}</button><button class="primary-button" type="button" disabled={!profile || selectedCount === 0 || operationBusy || !hasSelectedDownloadOptions(downloadOptions)} onclick={downloadBatch}><Download size={16} />{$t("batch.enqueue")}{selectedCount > 0 ? ` · ${selectedCount}` : ""}</button></div>
              </header>
              {#if profile}
                <div class="batch-controls-row">
                  <ContentOptions options={downloadOptions} onChange={(next) => (downloadOptions = next)} label={$t("batch.downloadContent")} />
                  <label class="batch-filter">
                    <Search size={13} />
                    <input type="search" bind:value={batchFilter} placeholder={$t("batch.filterPlaceholder")} aria-label={$t("batch.filterPlaceholder")} />
                    {#if batchFilter}<button class="batch-filter-clear" type="button" aria-label={$t("common.close")} onclick={() => (batchFilter = "")}><X size={12} /></button>{/if}
                  </label>
                </div>
              {/if}
              {#if profile && visibleProfileItems.length === 0}
                <div class="empty-list"><Search size={20} /><span>{$t("batch.noMatch")}「{batchFilter.trim()}」</span></div>
              {:else}
                <BatchList items={visibleProfileItems} selectedIds={selectedProfileIds} selectedFormats={selectedProfileFormats} onToggle={toggleProfileItem} onFormat={(id, formatId) => (selectedProfileFormats = { ...selectedProfileFormats, [id]: formatId })} />
              {/if}
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
