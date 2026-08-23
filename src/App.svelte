<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    ClipboardPaste,
    Download,
    FolderOpen,
    History,
    ListVideo,
    LoaderCircle,
    Menu,
    PanelRightClose,
    PanelRightOpen,
    Search,
    Settings,
    SlidersHorizontal,
    Sparkles,
    SquareStack,
    Trash2
  } from "@lucide/svelte";
  import AnalysisProgress from "./lib/components/AnalysisProgress.svelte";
  import BatchList from "./lib/components/BatchList.svelte";
  import ContentOptions from "./lib/components/ContentOptions.svelte";
  import PlatformIcon from "./lib/components/PlatformIcon.svelte";
  import SettingsSheet from "./lib/components/SettingsSheet.svelte";
  import SignalField from "./lib/components/SignalField.svelte";
  import SingleFormatList from "./lib/components/SingleFormatList.svelte";
  import TaskPanel from "./lib/components/TaskPanel.svelte";
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
  let workflowMode = $state<WorkflowMode>("single");
  let rawInput = $state("");
  let analyzing = $state(false);
  let analysisProgress = $state<AnalysisProgressState | null>(null);
  let operationBusy = $state(false);
  let notice = $state("");
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

  function selectPlatform(next: PlatformId) {
    if (next === platform) return;
    platform = next;
    if (next !== "youtube" && workflowMode === "playlist") workflowMode = "single";
    resetWorkspace();
  }

  function selectWorkflowMode(next: WorkflowMode) {
    if (next === workflowMode) return;
    workflowMode = next;
    resetWorkspace();
  }

  function resetWorkspace() {
    analysisGeneration += 1;
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
    notice = "";
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
      notice = `作品已解析，但缩略图加载失败：${lastError}`;
    }
  }

  async function hydrateYouTubeBatchFormats(batch: ProfileBatch, generation: number) {
    const pendingItems = batch.items.map((item) => ({ ...item, formatStatus: "pending" as const }));
    if (generation !== analysisGeneration) return 0;

    profile = { ...batch, items: pendingItems };
    selectedProfileFormats = {};
    analysisProgress = {
      current: 0,
      total: pendingItems.length,
      message: `正在读取真实清晰度（0/${pendingItems.length}）…`
    };

    let cursor = 0;
    let completed = 0;
    let failed = 0;
    const worker = async () => {
      while (generation === analysisGeneration) {
        const index = cursor;
        cursor += 1;
        const item = pendingItems[index];
        if (!item) return;

        let nextItem: VideoAsset;
        try {
          const resolved = await analyzeBatchItem(item.sourceUrl);
          const loaded = resolved.formats.length > 0;
          if (!loaded) failed += 1;
          nextItem = {
            ...item,
            ...resolved,
            categoryLabel: item.categoryLabel,
            groupTitle: item.groupTitle,
            formatStatus: loaded ? "loaded" : "failed"
          };
        } catch {
          failed += 1;
          nextItem = { ...item, formatStatus: "failed" };
        }

        if (generation !== analysisGeneration || !profile) return;
        completed += 1;
        profile = {
          ...profile,
          items: profile.items.map((candidate, candidateIndex) => candidateIndex === index ? nextItem : candidate)
        };
        const formats = visibleFormats(nextItem, "active");
        const selected = formats.find((format) => format.recommended)?.id ?? formats[0]?.id;
        if (selected) {
          selectedProfileFormats = { ...selectedProfileFormats, [nextItem.assetId]: selected };
        }
        analysisProgress = {
          current: completed,
          total: pendingItems.length,
          message: `正在读取真实清晰度（${completed}/${pendingItems.length}）…`
        };
      }
    };

    const workers = Math.min(4, pendingItems.length);
    await Promise.all(Array.from({ length: workers }, worker));
    return failed;
  }

  async function analyze() {
    if (!rawInput.trim()) return;
    const validationError = validateInputTarget(rawInput, platform, workflowMode);
    if (validationError) {
      errorMessage = validationError;
      notice = "";
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
    notice = "";
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
          const failed = await hydrateYouTubeBatchFormats(batch, generation);
          if (generation === analysisGeneration && failed > 0) {
            notice = `${batch.items.length - failed} 个视频已读取真实清晰度，${failed} 个读取失败，可重新解析后再试。`;
          }
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
      analysisProgress = null;
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
      notice = "任务已加入队列。";
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
      notice = result.message;
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

<svelte:head><meta name="theme-color" content="#101113" /></svelte:head>

<svelte:boundary onerror={(error) => (errorMessage = resolveErrorMessage(error))}>
  <div class="app-shell" data-platform={platform}>
    <SignalField paused={settingsOpen} />
    <aside class="nav-rail" aria-label={$t("app.mainNavigation")}>
      <button class="brand-mark" type="button" title="StreamVerse" aria-label={`StreamVerse ${$t("workspace.title")}`} onclick={() => (view = "download")}><Sparkles size={21} /></button>
      <nav>
        <button class:active={view === "download"} class="rail-button" type="button" title={$t("common.download")} aria-label={$t("common.download")} onclick={() => (view = "download")}><Download size={19} /></button>
        <button class:active={view === "history"} class="rail-button" type="button" title={$t("history.title")} aria-label={$t("history.title")} onclick={loadHistory}><History size={19} /></button>
      </nav>
      <button class="rail-button" type="button" title={$t("app.settings")} aria-label={$t("app.settings")} onclick={openSettings}><Settings size={19} /></button>
    </aside>

    <main class="main-stage">
      <header class="stage-header">
        <div class="wordmark"><span>STREAM</span><strong>VERSE</strong><small>1.0</small></div>
        <div class="stage-status"><span class:online={activeTaskCount > 0}></span><b>{activeTaskCount > 0 ? `${activeTaskCount} ACTIVE` : $t("app.systemReady")}</b></div>
        <button class="queue-toggle icon-button" type="button" title={queueCollapsed ? $t("task.expandQueue") : $t("task.collapseQueue")} aria-label={queueCollapsed ? $t("task.expandQueue") : $t("task.collapseQueue")} onclick={() => (queueCollapsed = !queueCollapsed)}>{#if queueCollapsed}<PanelRightOpen size={18} />{:else}<PanelRightClose size={18} />{/if}</button>
      </header>

      {#if view === "history"}
        <section class="history-workspace">
          <header><div><span class="eyebrow">ARCHIVE INDEX</span><h1>{$t("history.title")}</h1></div><strong>{history.length.toString().padStart(3, "0")}</strong></header>
          {#if historyLoading}<div class="center-loader"><LoaderCircle class="spin" size={22} /></div>{:else}
            <div class="history-list">
              {#each history as entry, index (entry.platform + entry.assetId)}
                <article><span>{String(index + 1).padStart(3, "0")}</span><b style:--platform-color={platformMeta[entry.platform].color}>{platformMeta[entry.platform].code}</b><strong>{entry.title}</strong><time>{entry.downloadedAt}</time></article>
              {:else}<div class="empty-state">{$t("history.empty")}</div>{/each}
            </div>
          {/if}
        </section>
      {:else}
        <section class="download-workspace">
          <div class="workspace-heading">
            <div><span class="eyebrow">SIGNAL IN / FILE OUT</span><h1>{$t("workspace.title")}</h1></div>
            <div class="platform-switch" role="tablist" aria-label="平台">
              {#each Object.keys(platformMeta) as id}
                <button class:active={platform === id} style:--platform-color={platformMeta[id as PlatformId].color} type="button" role="tab" aria-selected={platform === id} onclick={() => selectPlatform(id as PlatformId)}><PlatformIcon platform={id as PlatformId} size={14} />{platformMeta[id as PlatformId].label}</button>
              {/each}
            </div>
          </div>

          <div class="input-console">
            <div class="mode-switch" role="tablist" aria-label={$t("workspace.mode")}><button class:active={workflowMode === "single"} type="button" role="tab" onclick={() => selectWorkflowMode("single")}><Download size={15} />{$t("workspace.single")}</button><button class:active={workflowMode === "profile"} type="button" role="tab" onclick={() => selectWorkflowMode("profile")}><SquareStack size={15} />{platform === "youtube" ? $t("workspace.youtubeChannel") : $t("workspace.profile")}</button>{#if platform === "youtube"}<button class:active={workflowMode === "playlist"} type="button" role="tab" onclick={() => selectWorkflowMode("playlist")}><ListVideo size={15} />{$t("workspace.youtubePlaylist")}</button>{/if}</div>
            <div class="signal-input"><textarea bind:value={rawInput} rows="3" aria-label={$t("workspace.inputLabel")} placeholder={$t("workspace.placeholder")} onkeydown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") analyze(); }}></textarea><button class="icon-button paste-button" type="button" title={$t("workspace.paste")} aria-label={$t("workspace.paste")} onclick={pasteInput}><ClipboardPaste size={18} /></button><button class="analyze-button" type="button" disabled={analyzing || !rawInput.trim()} onclick={analyze}>{#if analyzing}<LoaderCircle class="spin" size={18} />{:else}<Search size={18} />{/if}<span>{analyzing ? $t("common.analyzing") : $t("common.analyze")}</span></button></div>
            <div class="console-meta"><span style:--platform-color={platformMeta[platform].color}>{platformMeta[platform].code} / {workflowMode === "single" ? "SINGLE" : workflowMode === "playlist" ? "PLAYLIST" : "CHANNEL"}</span><span class:active-auth={authStatus === "active"}>{authStatus === "active" ? "AUTH ACTIVE" : "GUEST MODE"}</span><button type="button" onclick={openSettings}><SlidersHorizontal size={14} />{bootstrap?.saveDirectory ?? "--"}</button></div>
          </div>

          {#if analyzing && analysisProgress}<AnalysisProgress progress={analysisProgress} />{/if}

          {#if errorMessage}<div class="message-strip error" role="alert">{errorMessage}</div>{/if}
          {#if notice}<div class="message-strip success" aria-live="polite">{notice}</div>{/if}

          {#if workflowMode === "single"}
            {#if preview}
              <div class="single-result">
                <figure>{#if previewCoverUrl && !previewCoverFailed}<img src={previewCoverUrl} alt={preview.title} width="960" height="540" decoding="async" onerror={() => (previewCoverFailed = true)} />{:else}<div class="cover-placeholder"><PlatformIcon platform={preview.platform} size={52} /></div>{/if}<figcaption><span>{formatDuration(preview.durationSeconds)}</span></figcaption></figure>
                <div class="result-detail"><span class="eyebrow">{platformMeta[preview.platform].label} / {preview.author}</span><h2>{preview.title}</h2><p>{preview.publishDate || "--"}</p>{#if previewIsAlbum}<div class="album-summary"><strong>{$t("content.album")}</strong><span>{preview.imageUrls?.length ?? 0} {$t("content.images")}</span></div>{:else}<SingleFormatList formats={previewFormats} selectedId={selectedFormatId} expanded={formatsExpanded} durationSeconds={preview.durationSeconds} onSelect={(formatId) => (selectedFormatId = formatId)} onExpandedChange={(expanded) => (formatsExpanded = expanded)} />{/if}<ContentOptions options={downloadOptions} onChange={(next) => (downloadOptions = next)} label={$t("single.downloadContent")} /><button class="download-button" type="button" disabled={operationBusy || !canDownloadSingle} onclick={downloadSingle}>{#if operationBusy}<LoaderCircle class="spin" size={18} />{:else}<Download size={18} />{/if}{$t("workspace.enqueue")}</button></div>
              </div>
            {:else}<div class="idle-stage"><span>01</span><strong>AWAITING SIGNAL</strong></div>{/if}
          {:else}
            <div class="batch-workspace">
              <header><div><span class="eyebrow">{profile?.profileTitle ?? "BATCH SELECTOR"}</span><strong>{profile?.items.length ?? 0} ITEMS / {selectedCount} SELECTED</strong></div><div><button class="quiet-button" type="button" disabled={!profile} onclick={selectAllProfileItems}><Check size={15} />全选</button><button class="quiet-button" type="button" disabled={!profile} onclick={invertProfileSelection}><Menu size={15} />反选</button><button class="primary-button" type="button" disabled={!profile || selectedCount === 0 || operationBusy || !hasSelectedDownloadOptions(downloadOptions)} onclick={downloadBatch}><Download size={16} />下载 {selectedCount}</button></div></header>
              {#if profile}<ContentOptions options={downloadOptions} onChange={(next) => (downloadOptions = next)} label={$t("batch.downloadContent")} />{/if}
              <BatchList items={profile?.items ?? []} selectedIds={selectedProfileIds} selectedFormats={selectedProfileFormats} onToggle={toggleProfileItem} onFormat={(id, formatId) => (selectedProfileFormats = { ...selectedProfileFormats, [id]: formatId })} />
            </div>
          {/if}
        </section>
      {/if}
    </main>

    <TaskPanel tasks={bootstrap?.tasks ?? []} collapsed={queueCollapsed} onControl={handleTaskControl} onReveal={(task) => task.outputPath && openInFileManager(task.outputPath, true)} onRemove={async (task) => { await removeDownloadTask(task.id); applyTaskEvent({ type: "delete", taskId: task.id }); }} onClear={async () => { if (!bootstrap) return; bootstrap.tasks = await clearFinishedTasks(); }} />

    {#if bootstrap}
      <SettingsSheet open={settingsOpen} {bootstrap} {browserSources} busy={operationBusy} onClose={() => (settingsOpen = false)} onSave={handleSaveSettings} onPickDirectory={() => pickSaveDirectory(bootstrap!.saveDirectory)} onPickCookieFile={pickCookieFile} onImportBrowser={handleImportBrowser} onSaveManual={async (platformId, value) => { const result = await saveManualCookies(platformId, { cookieText: value }); bootstrap!.platformAuth[platformId] = { mode: "manual", status: "active", consentedAt: Math.floor(Date.now() / 1000) }; return result; }} onImportCookieFile={async (platformId, path) => { const result = await saveManualCookies(platformId, { cookieFile: path }); bootstrap!.platformAuth[platformId] = { mode: "manual", status: "active", consentedAt: Math.floor(Date.now() / 1000) }; return result; }} onClearAuth={async (platformId) => { await clearPlatformAuth(platformId); bootstrap!.platformAuth[platformId] = { mode: "none", status: "guest" }; }} />
    {/if}
  </div>

  {#snippet failed(error, reset)}
    <div class="fatal-boundary"><strong>界面渲染失败</strong><p>{resolveErrorMessage(error)}</p><button class="primary-button" type="button" onclick={reset}>重新加载</button></div>
  {/snippet}
</svelte:boundary>
