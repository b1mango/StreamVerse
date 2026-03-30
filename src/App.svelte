<script lang="ts">
  import { onMount } from "svelte";
  import {
    analyzeInput,
    analyzeProfileInput,
    cancelDownloadTask,
    clearFinishedTasks,
    createDownloadTask,
    createProfileDownloadTasks,
    getBootstrapState,
    listDownloadTasks,
    openInFileManager,
    pauseDownloadTask,
    pickSaveDirectory,
    resumeDownloadTask,
    saveSettings
  } from "./lib/backend";
  import type {
    AuthState,
    BootstrapState,
    DownloadContentSelection,
    DownloadMode,
    DownloadTask,
    ProfileBatch,
    QualityPreference,
    SettingsProfile,
    VideoAsset,
    VideoFormat
  } from "./lib/types";

  type WorkspacePage = "single" | "profile";

  let loading = true;
  let analyzingSingle = false;
  let downloadingSingle = false;
  let analyzingProfile = false;
  let batchEnqueuing = false;
  let pastingSingle = false;
  let pastingProfile = false;
  let clearingFinished = false;
  let settingsOpen = false;
  let settingsSaving = false;
  let pickingDirectory = false;
  let pickingTargetDirectory = false;
  let openingFolder = false;
  let activePage: WorkspacePage = "single";
  let bootstrap: BootstrapState | null = null;
  let singlePreview: VideoAsset | null = null;
  let profilePreview: ProfileBatch | null = null;
  let tasks: DownloadTask[] = [];
  let singleInput = "";
  let profileInput = "";
  let selectedFormatId = "";
  let profileBatchLimit = 24;
  let errorMessage = "";
  let successMessage = "";
  let cookieBrowser = "";
  let saveDirectoryDraft = "";
  let targetDirectory = "";
  let downloadMode: DownloadMode = "manual";
  let qualityPreference: QualityPreference = "recommended";
  let autoRevealInFinder = false;
  let selectedProfileIds: string[] = [];
  let taskActionPendingIds: string[] = [];
  let pollTimer: number | undefined;
  let singleDownloadOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let profileDownloadOptions: DownloadContentSelection =
    createDefaultDownloadOptions();

  const isDesktopRuntime =
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const browserOptions = [
    { value: "", label: "未登录" },
    { value: "chrome", label: "Chrome" },
    { value: "safari", label: "Safari" },
    { value: "firefox", label: "Firefox" },
    { value: "edge", label: "Edge" },
    { value: "brave", label: "Brave" }
  ];

  const downloadModeOptions: Array<{ value: DownloadMode; label: string }> = [
    { value: "manual", label: "手动模式" },
    { value: "smart", label: "智能模式" }
  ];

  const qualityOptions: Array<{ value: QualityPreference; label: string }> = [
    { value: "recommended", label: "推荐优先" },
    { value: "highest", label: "最高质量" },
    { value: "no_watermark", label: "无水印优先" },
    { value: "smallest", label: "最小体积" }
  ];

  const pageOptions: Array<{
    value: WorkspacePage;
    label: string;
    summary: string;
  }> = [
    {
      value: "single",
      label: "单视频下载",
      summary: "针对单条分享链接，先解析，再选内容和清晰度。"
    },
    {
      value: "profile",
      label: "主页批量下载",
      summary: "先读取主页作品，再勾选需要下载的内容与视频。"
    }
  ];

  const authMap: Record<AuthState, string> = {
    guest: "游客模式",
    active: "已登录",
    expired: "登录失效"
  };

  const taskLabelMap: Record<DownloadTask["status"], string> = {
    idle: "空闲",
    analyzing: "解析中",
    queued: "排队中",
    downloading: "下载中",
    paused: "已暂停",
    cancelled: "已取消",
    completed: "已完成",
    failed: "失败"
  };

  const modeLabelMap: Record<DownloadMode, string> = {
    manual: "手动下载",
    smart: "智能下载"
  };

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
    syncSettings(bootstrap);

    if (!isDesktopRuntime) {
      singlePreview = bootstrap.preview;
      singleInput = bootstrap.preview.sourceUrl;
      selectedFormatId = pickPreferredFormat(bootstrap.preview)?.id ?? "";
    }

    if (isDesktopRuntime) {
      pollTimer = window.setInterval(async () => {
        tasks = await listDownloadTasks();
      }, 1200);
    }

    loading = false;
  }

  function createDefaultDownloadOptions(): DownloadContentSelection {
    return {
      downloadVideo: true,
      downloadCover: false,
      downloadCaption: false,
      downloadMetadata: false
    };
  }

  function syncSettings(next: BootstrapState | SettingsProfile) {
    cookieBrowser = next.cookieBrowser ?? "";
    saveDirectoryDraft = next.saveDirectory;
    targetDirectory = next.saveDirectory;
    downloadMode = next.downloadMode;
    qualityPreference = next.qualityPreference;
    autoRevealInFinder = next.autoRevealInFinder;
  }

  async function handleAnalyzeSingle() {
    analyzingSingle = true;
    errorMessage = "";
    successMessage = "";

    try {
      const nextPreview = await analyzeInput({ rawInput: singleInput });
      singlePreview = nextPreview;
      selectedFormatId = pickPreferredFormat(nextPreview)?.id ?? "";

      if (downloadMode === "smart" && singleDownloadOptions.downloadVideo) {
        const preferredFormat = pickPreferredFormat(nextPreview);
        if (preferredFormat) {
          await startSingleDownload(nextPreview, preferredFormat, true);
          return;
        }
      }

      successMessage = "作品解析完成，可以开始单条下载。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingSingle = false;
    }
  }

  async function handleAnalyzeProfile() {
    analyzingProfile = true;
    errorMessage = "";
    successMessage = "";

    try {
      const result = await analyzeProfileInput({
        rawInput: profileInput,
        limit: clampBatchLimit(profileBatchLimit)
      });
      profilePreview = result;
      selectedProfileIds = result.items.map((item) => item.awemeId);
      successMessage = "主页作品已载入，请勾选后再加入下载队列。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingProfile = false;
    }
  }

  async function handlePasteIntoPage(page: WorkspacePage) {
    if (page === "single") {
      pastingSingle = true;
    } else {
      pastingProfile = true;
    }

    errorMessage = "";
    successMessage = "";

    try {
      if (typeof navigator === "undefined" || !navigator.clipboard?.readText) {
        throw new Error("当前环境不支持直接读取剪贴板。");
      }

      const text = (await navigator.clipboard.readText()).trim();
      if (!text) {
        throw new Error("剪贴板里没有可解析的内容。");
      }

      if (page === "single") {
        singleInput = text;
        await handleAnalyzeSingle();
      } else {
        profileInput = text;
        await handleAnalyzeProfile();
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      if (page === "single") {
        pastingSingle = false;
      } else {
        pastingProfile = false;
      }
    }
  }

  async function handleCreateSingleTask() {
    if (!singlePreview) {
      return;
    }

    if (!hasSelectedDownloadOptions(singleDownloadOptions)) {
      errorMessage = "至少要选择一种要保存的内容。";
      return;
    }

    const format = singleDownloadOptions.downloadVideo
      ? visibleFormats(singlePreview).find((item) => item.id === selectedFormatId)
      : undefined;

    if (singleDownloadOptions.downloadVideo && !format) {
      errorMessage = "请先选择一个可用清晰度。";
      return;
    }

    await startSingleDownload(singlePreview, format, false);
  }

  async function startSingleDownload(
    asset: VideoAsset,
    format: VideoFormat | undefined,
    launchedBySmartMode: boolean
  ) {
    downloadingSingle = true;
    errorMessage = "";
    successMessage = "";

    try {
      const task = await createDownloadTask({
        awemeId: asset.awemeId,
        sourceUrl: asset.sourceUrl,
        title: asset.title,
        author: asset.author,
        publishDate: asset.publishDate,
        caption: asset.caption,
        coverUrl: asset.coverUrl ?? null,
        formatId: format?.id ?? null,
        formatLabel: format?.label ?? null,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions: singleDownloadOptions,
        directUrl: format?.directUrl ?? null,
        referer: format?.referer ?? null,
        userAgent: format?.userAgent ?? null
      });

      upsertTask(task);
      successMessage = launchedBySmartMode
        ? "作品解析完成，已按默认策略直接创建下载任务。"
        : task.message ?? "下载任务已开始。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      downloadingSingle = false;
    }
  }

  async function handleEnqueueProfileTasks() {
    if (!profilePreview) {
      return;
    }

    const items = profilePreview.items.filter((item) =>
      selectedProfileIds.includes(item.awemeId)
    );

    if (!items.length) {
      errorMessage = "请至少勾选一个主页作品。";
      return;
    }

    if (!hasSelectedDownloadOptions(profileDownloadOptions)) {
      errorMessage = "至少要选择一种要保存的内容。";
      return;
    }

    batchEnqueuing = true;
    errorMessage = "";
    successMessage = "";

    try {
      const result = await createProfileDownloadTasks({
        profileTitle: profilePreview.profileTitle,
        sourceUrl: profilePreview.sourceUrl,
        items,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions: profileDownloadOptions
      });
      tasks = await listDownloadTasks();
      successMessage = result.message;
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      batchEnqueuing = false;
    }
  }

  async function handleTaskControl(
    task: DownloadTask,
    action: "pause" | "resume" | "cancel"
  ) {
    taskActionPendingIds = [...taskActionPendingIds, task.id];
    errorMessage = "";

    try {
      const nextTask =
        action === "pause"
          ? await pauseDownloadTask(task.id)
          : action === "resume"
            ? await resumeDownloadTask(task.id)
            : await cancelDownloadTask(task.id);
      upsertTask(nextTask);
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
    errorMessage = "";
    successMessage = "";

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
        autoRevealInFinder: nextSettings.autoRevealInFinder
      };
      syncSettings(nextSettings);
      settingsOpen = false;
      successMessage = "设置已保存。新的解析和下载任务会使用这些默认值。";
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

  function switchPage(nextPage: WorkspacePage) {
    activePage = nextPage;
    errorMessage = "";
    successMessage = "";
  }

  function toggleProfileSelection(awemeId: string, checked: boolean) {
    selectedProfileIds = checked
      ? Array.from(new Set([...selectedProfileIds, awemeId]))
      : selectedProfileIds.filter((id) => id !== awemeId);
  }

  function selectAllProfileItems() {
    selectedProfileIds = profilePreview?.items.map((item) => item.awemeId) ?? [];
  }

  function clearProfileSelection() {
    selectedProfileIds = [];
  }

  function closeProfileSelection() {
    profilePreview = null;
    selectedProfileIds = [];
  }

  function upsertTask(task: DownloadTask) {
    tasks = [task, ...tasks.filter((item) => item.id !== task.id)];
  }

  function resolvedTargetDirectory() {
    return targetDirectory.trim() || bootstrap?.saveDirectory || "";
  }

  function hasSelectedDownloadOptions(options: DownloadContentSelection) {
    return (
      options.downloadVideo ||
      options.downloadCover ||
      options.downloadCaption ||
      options.downloadMetadata
    );
  }

  function summarizeDownloadOptions(options: DownloadContentSelection) {
    return [
      options.downloadVideo ? "视频" : null,
      options.downloadCover ? "封面" : null,
      options.downloadCaption ? "文案" : null,
      options.downloadMetadata ? "元数据" : null
    ]
      .filter(Boolean)
      .join(" / ");
  }

  function visibleFormats(asset: VideoAsset | null) {
    const formats = dedupeVisibleFormats(asset?.formats ?? []);
    if (bootstrap?.authState === "active") {
      return formats;
    }

    const publicFormats = formats.filter((item) => !item.requiresLogin);
    return publicFormats.length ? publicFormats : formats;
  }

  function pickPreferredFormat(asset: VideoAsset | null) {
    const candidateFormats = visibleFormats(asset);
    const rankedFormats = [...candidateFormats].sort((left, right) => {
      const heightDelta = formatHeight(right) - formatHeight(left);
      if (heightDelta !== 0) {
        return heightDelta;
      }

      return right.bitrateKbps - left.bitrateKbps;
    });

    switch (qualityPreference) {
      case "highest":
        return rankedFormats[0];
      case "smallest":
        return rankedFormats.at(-1) ?? rankedFormats[0];
      case "no_watermark":
        return (
          rankedFormats.find((item) => item.noWatermark) ??
          rankedFormats.find((item) => item.recommended) ??
          rankedFormats[0]
        );
      case "recommended":
      default:
        return (
          candidateFormats.find((item) => item.recommended) ?? rankedFormats[0]
        );
    }
  }

  function selectedFormat(asset: VideoAsset | null): VideoFormat | undefined {
    return visibleFormats(asset).find((item) => item.id === selectedFormatId);
  }

  function formatHeight(format: VideoFormat) {
    return Number.parseInt(format.resolution.split("x")[1] ?? "0", 10) || 0;
  }

  function normalizeFormatKey(value: string) {
    return value
      .trim()
      .toUpperCase()
      .replace(/[^A-Z0-9]/g, "");
  }

  function dedupeVisibleFormats(formats: VideoFormat[]) {
    const deduped = new Map<string, VideoFormat>();

    for (const format of formats) {
      const key = [
        normalizeFormatKey(format.label),
        normalizeFormatKey(format.resolution),
        normalizeFormatKey(format.codec),
        normalizeFormatKey(format.container),
        format.noWatermark ? "NOWM" : "WM",
        format.requiresLogin ? "LOGIN" : "PUBLIC"
      ].join("|");
      const existing = deduped.get(key);

      if (!existing) {
        deduped.set(key, format);
        continue;
      }

      const shouldReplace =
        (format.recommended && !existing.recommended) ||
        (Boolean(format.directUrl) && !existing.directUrl) ||
        format.bitrateKbps > existing.bitrateKbps;

      if (shouldReplace) {
        deduped.set(key, format);
      }
    }

    return Array.from(deduped.values());
  }

  function isProfileItemSelected(awemeId: string) {
    return selectedProfileIds.includes(awemeId);
  }

  function pendingTaskAction(taskId: string) {
    return taskActionPendingIds.includes(taskId);
  }

  function selectedBatchCount() {
    return selectedProfileIds.length;
  }

  function formatDuration(totalSeconds: number) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes.toString().padStart(2, "0")}:${seconds
      .toString()
      .padStart(2, "0")}`;
  }

  function finishedTaskCount(items: DownloadTask[]) {
    return items.filter((task) =>
      ["completed", "failed", "cancelled"].includes(task.status)
    ).length;
  }

  function clampBatchLimit(value: number) {
    if (!Number.isFinite(value)) {
      return 24;
    }

    return Math.max(1, Math.min(100, Math.round(value)));
  }

  function resolveErrorMessage(error: unknown) {
    if (error instanceof Error) {
      return error.message;
    }

    if (typeof error === "string") {
      return error;
    }

    return "操作失败，请稍后再试。";
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
          <h1>下载工作台</h1>
          <p class="status-copy">
            {authMap[bootstrap.authState]} · {modeLabelMap[bootstrap.downloadMode]}
          </p>
        </div>

        <div class="topbar-actions">
          <button
            class="ghost-button"
            onclick={handleOpenCurrentDirectory}
            disabled={openingFolder}
          >
            {openingFolder ? "打开中…" : "打开目录"}
          </button>
          <button class="ghost-button" onclick={handleOpenSettings}>设置</button>
        </div>
      </header>

      <section class="page-nav">
        {#each pageOptions as page}
          <button
            class:active={activePage === page.value}
            class="page-tab"
            onclick={() => switchPage(page.value)}
            type="button"
          >
            <strong>{page.label}</strong>
            <span>{page.summary}</span>
          </button>
        {/each}
      </section>

      <div class="shared-strip">
        <div class="path-copy">
          <span>当前保存位置</span>
          <strong title={resolvedTargetDirectory()}>{resolvedTargetDirectory()}</strong>
        </div>

        <div class="path-actions">
          <button
            class="ghost-button"
            onclick={handlePickTargetDirectory}
            disabled={pickingTargetDirectory || batchEnqueuing || downloadingSingle}
          >
            {pickingTargetDirectory ? "选择中…" : "更改目录"}
          </button>
          <button
            class="ghost-button"
            onclick={() => (targetDirectory = bootstrap!.saveDirectory)}
            disabled={resolvedTargetDirectory() === bootstrap!.saveDirectory}
          >
            恢复默认
          </button>
        </div>
      </div>

      {#if errorMessage}
        <p class="notice error">{errorMessage}</p>
      {/if}

      {#if successMessage}
        <p class="notice success">{successMessage}</p>
      {/if}

      {#if activePage === "single"}
        <section class="page-shell">
          <article class="panel page-hero">
            <div class="composer-copy">
              <p class="eyebrow">Single Video</p>
              <h2>单条视频下载</h2>
              <p class="lede">
                单视频页只关注一条分享链接：解析作品、选择下载内容、确认清晰度，然后立即开始下载。
              </p>
            </div>

            <label class="input-wrap">
              <textarea
                bind:value={singleInput}
                rows="4"
                placeholder="粘贴抖音分享文案、短链或作品页链接"
              ></textarea>
            </label>

            <div class="selection-block">
              <div class="section-head compact">
                <div>
                  <p class="eyebrow">Download Items</p>
                  <h3>下载内容</h3>
                </div>
                <span class="chip subtle">
                  {hasSelectedDownloadOptions(singleDownloadOptions)
                    ? summarizeDownloadOptions(singleDownloadOptions)
                    : "未选择"}
                </span>
              </div>

              <div class="option-grid">
                <label class="option-chip">
                  <input bind:checked={singleDownloadOptions.downloadVideo} type="checkbox" />
                  <span>视频</span>
                </label>
                <label class="option-chip">
                  <input bind:checked={singleDownloadOptions.downloadCover} type="checkbox" />
                  <span>封面</span>
                </label>
                <label class="option-chip">
                  <input bind:checked={singleDownloadOptions.downloadCaption} type="checkbox" />
                  <span>文案</span>
                </label>
                <label class="option-chip subtle-option">
                  <input bind:checked={singleDownloadOptions.downloadMetadata} type="checkbox" />
                  <span>元数据 JSON</span>
                </label>
              </div>

              <div class="option-notes">
                <span class="meta-item">只勾选单项时直接保存单文件。</span>
                <span class="meta-item">勾选多项时自动创建以标题命名的文件夹。</span>
                <span class="meta-item">`JSON` 只用于保存结构化元数据，默认可不选。</span>
              </div>
            </div>

            <div class="action-row">
              <button
                class="primary-button"
                onclick={handleAnalyzeSingle}
                disabled={analyzingSingle || downloadingSingle || pastingSingle}
              >
                {analyzingSingle ? "解析中…" : "解析作品"}
              </button>
              <button
                class="secondary-button"
                onclick={() => handlePasteIntoPage("single")}
                disabled={analyzingSingle || downloadingSingle || pastingSingle}
              >
                {pastingSingle ? "读取中…" : "粘贴并解析"}
              </button>
              <button
                class="secondary-button"
                onclick={handleCreateSingleTask}
                disabled={!singlePreview || analyzingSingle || downloadingSingle}
              >
                {downloadingSingle ? "创建任务…" : "开始下载"}
              </button>
            </div>

            <div class="meta-row">
              <span class="meta-item">
                默认策略：{qualityOptions.find((item) => item.value === qualityPreference)?.label}
              </span>
              <span class="meta-item">解析器：自动（抖音桥接 / yt-dlp）</span>
              <span class="meta-item">
                当前格式：{selectedFormat(singlePreview)?.label ?? "等待解析"}
              </span>
            </div>
          </article>

          {#if singlePreview}
            <section class="analysis-grid">
              <article class="panel preview-panel">
                <p class="eyebrow">Preview</p>
                <h3>{singlePreview.title}</h3>
                <p class="preview-caption">{singlePreview.caption}</p>

                <div class="facts">
                  <div>
                    <span>作者</span>
                    <strong>{singlePreview.author}</strong>
                  </div>
                  <div>
                    <span>时长</span>
                    <strong>{formatDuration(singlePreview.durationSeconds)}</strong>
                  </div>
                  <div>
                    <span>发布日期</span>
                    <strong>{singlePreview.publishDate}</strong>
                  </div>
                </div>
              </article>

              <article class="panel formats-panel">
                <div class="section-head">
                  <div>
                    <p class="eyebrow">Formats</p>
                    <h3>选择清晰度</h3>
                  </div>
                  <span class="chip subtle">{visibleFormats(singlePreview).length} 个格式</span>
                </div>

                {#if singleDownloadOptions.downloadVideo}
                  <div class="format-list">
                    {#each visibleFormats(singlePreview) as format}
                      <button
                        class:selected={selectedFormatId === format.id}
                        class="format-row"
                        onclick={() => (selectedFormatId = format.id)}
                        type="button"
                      >
                        <div class="format-copy">
                          <strong>{format.label}</strong>
                          <span>
                            {format.resolution} · {format.codec} · {format.container}
                          </span>
                        </div>

                        <div class="format-tags">
                          {#if format.recommended}
                            <span class="mini-tag accent">推荐</span>
                          {/if}
                          {#if format.noWatermark}
                            <span class="mini-tag">无水印</span>
                          {/if}
                          {#if format.requiresLogin}
                            <span class="mini-tag">登录后</span>
                          {/if}
                        </div>
                      </button>
                    {/each}
                  </div>
                {:else}
                  <p class="empty-state">
                    当前只保存附加内容，所以这里不需要选择清晰度。
                  </p>
                {/if}
              </article>
            </section>
          {/if}
        </section>
      {:else}
        <section class="page-shell">
          <article class="panel page-hero">
            <div class="composer-copy">
              <p class="eyebrow">Profile Batch</p>
              <h2>主页批量下载</h2>
              <p class="lede">
                主页批量页先读取主页作品列表，再让你勾选想下载的视频和附加内容，最后统一入队。
              </p>
            </div>

            <label class="input-wrap">
              <textarea
                bind:value={profileInput}
                rows="4"
                placeholder="粘贴抖音个人主页分享文案或主页链接"
              ></textarea>
            </label>

            <div class="profile-topline">
              <label class="inline-count-field">
                <span>主页批量上限</span>
                <input
                  bind:value={profileBatchLimit}
                  class="inline-count-input"
                  max="100"
                  min="1"
                  type="number"
                />
              </label>

              <div class="meta-row page-meta">
                <span class="meta-item">
                  批量会按“{qualityOptions.find((item) => item.value === qualityPreference)?.label}”自动选格式
                </span>
              </div>
            </div>

            <div class="selection-block">
              <div class="section-head compact">
                <div>
                  <p class="eyebrow">Batch Items</p>
                  <h3>批量下载内容</h3>
                </div>
                <span class="chip subtle">
                  {hasSelectedDownloadOptions(profileDownloadOptions)
                    ? summarizeDownloadOptions(profileDownloadOptions)
                    : "未选择"}
                </span>
              </div>

              <div class="option-grid">
                <label class="option-chip">
                  <input bind:checked={profileDownloadOptions.downloadVideo} type="checkbox" />
                  <span>视频</span>
                </label>
                <label class="option-chip">
                  <input bind:checked={profileDownloadOptions.downloadCover} type="checkbox" />
                  <span>封面</span>
                </label>
                <label class="option-chip">
                  <input bind:checked={profileDownloadOptions.downloadCaption} type="checkbox" />
                  <span>文案</span>
                </label>
                <label class="option-chip subtle-option">
                  <input
                    bind:checked={profileDownloadOptions.downloadMetadata}
                    type="checkbox"
                  />
                  <span>元数据 JSON</span>
                </label>
              </div>

              <div class="option-notes">
                <span class="meta-item">只保留视频时不会额外建文件夹。</span>
                <span class="meta-item">勾选多项内容后，每个作品都会创建自己的文件夹。</span>
              </div>
            </div>

            <div class="action-row">
              <button
                class="primary-button"
                onclick={handleAnalyzeProfile}
                disabled={analyzingProfile || batchEnqueuing || pastingProfile}
              >
                {analyzingProfile ? "读取主页…" : "读取主页作品"}
              </button>
              <button
                class="secondary-button"
                onclick={() => handlePasteIntoPage("profile")}
                disabled={analyzingProfile || batchEnqueuing || pastingProfile}
              >
                {pastingProfile ? "读取中…" : "粘贴并读取"}
              </button>
              <button
                class="secondary-button"
                onclick={handleEnqueueProfileTasks}
                disabled={!profilePreview || batchEnqueuing || !selectedBatchCount()}
              >
                {batchEnqueuing ? "加入队列中…" : "将所选作品加入队列"}
              </button>
            </div>
          </article>

          {#if profilePreview}
            <article class="panel profile-panel">
              <div class="section-head">
                <div>
                  <p class="eyebrow">Profile Result</p>
                  <h3>{profilePreview.profileTitle}</h3>
                </div>

                <div class="section-actions">
                  <span class="chip subtle">
                    已选 {selectedBatchCount()} / {profilePreview.items.length}
                  </span>
                  <button class="text-button" onclick={selectAllProfileItems} type="button">
                    全选
                  </button>
                  <button class="text-button" onclick={clearProfileSelection} type="button">
                    清空
                  </button>
                  <button class="text-button" onclick={closeProfileSelection} type="button">
                    关闭
                  </button>
                </div>
              </div>

              <div class="meta-row">
                <span class="meta-item">本次读取 {profilePreview.fetchedCount} 个作品</span>
                <span class="meta-item">勾选后的作品会统一使用当前批量下载内容设置</span>
              </div>

              <div class="profile-list">
                {#each profilePreview.items as item}
                  <label class="profile-row">
                    <input
                      checked={isProfileItemSelected(item.awemeId)}
                      onchange={(event) =>
                        toggleProfileSelection(
                          item.awemeId,
                          (event.currentTarget as HTMLInputElement).checked
                        )}
                      type="checkbox"
                    />

                    <div class="profile-copy">
                      <strong>{item.title}</strong>
                      <span>
                        {item.publishDate} · {formatDuration(item.durationSeconds)} ·
                        {pickPreferredFormat(item)?.label ?? "无视频格式"}
                      </span>
                    </div>
                  </label>
                {/each}
              </div>
            </article>
          {/if}
        </section>
      {/if}

      <article class="panel tasks-panel">
        <div class="section-head">
          <div>
            <p class="eyebrow">Recent Tasks</p>
            <h3>最近任务</h3>
          </div>

          <div class="section-actions">
            {#if finishedTaskCount(tasks) > 0}
              <button
                class="text-button"
                onclick={handleClearFinished}
                disabled={clearingFinished}
              >
                {clearingFinished ? "清理中…" : "清理已完成"}
              </button>
            {/if}
            <span class="chip subtle">{tasks.length} 个</span>
          </div>
        </div>

        {#if tasks.length === 0}
          <p class="empty-state">还没有下载任务。先在上面的页面里创建一个试试。</p>
        {:else}
          <div class="task-list">
            {#each tasks as task}
              <div class="task-row">
                <div class="task-copy">
                  <strong>{task.title}</strong>
                  <span>{task.formatLabel}</span>
                  <div class="task-progress">
                    <div
                      class:completed={task.status === "completed"}
                      class:failed={task.status === "failed" || task.status === "cancelled"}
                      class="task-progress-fill"
                      style={`width: ${task.progress}%`}
                    ></div>
                  </div>
                  {#if task.message}
                    <small>{task.message}</small>
                  {/if}
                  {#if task.outputPath}
                    <small>{task.outputPath}</small>
                  {/if}
                </div>

                <div class="task-side">
                  <span>{task.progress}%</span>
                  <strong>{taskLabelMap[task.status]}</strong>
                  <small>{task.speedText} · ETA {task.etaText}</small>

                  <div class="task-actions">
                    {#if task.status === "downloading" && task.supportsPause}
                      <button
                        class="text-button"
                        onclick={() => handleTaskControl(task, "pause")}
                        disabled={pendingTaskAction(task.id)}
                        type="button"
                      >
                        暂停
                      </button>
                    {/if}

                    {#if task.status === "paused" && task.supportsPause}
                      <button
                        class="text-button"
                        onclick={() => handleTaskControl(task, "resume")}
                        disabled={pendingTaskAction(task.id)}
                        type="button"
                      >
                        继续
                      </button>
                    {/if}

                    {#if ["queued", "downloading", "paused"].includes(task.status) && task.supportsCancel}
                      <button
                        class="text-button danger"
                        onclick={() => handleTaskControl(task, "cancel")}
                        disabled={pendingTaskAction(task.id)}
                        type="button"
                      >
                        取消
                      </button>
                    {/if}

                    {#if task.outputPath}
                      <button
                        class="text-button"
                        onclick={() => handleRevealTask(task)}
                        type="button"
                      >
                        定位文件
                      </button>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </article>

      {#if settingsOpen}
        <button
          aria-label="关闭设置"
          class="settings-overlay"
          onclick={() => (settingsOpen = false)}
          type="button"
        ></button>
        <aside class="panel settings-panel">
          <div class="section-head">
            <div>
              <p class="eyebrow">Settings</p>
              <h3>应用设置</h3>
            </div>
            <button class="ghost-button" onclick={() => (settingsOpen = false)}>
              关闭
            </button>
          </div>

          <div class="settings-stack">
            <label class="settings-field">
              <span class="settings-label">默认下载路径</span>
              <input
                bind:value={saveDirectoryDraft}
                class="settings-input"
                placeholder="选择或输入默认下载目录"
                type="text"
              />
            </label>

            <div class="settings-actions">
              <button
                class="secondary-button"
                onclick={handlePickSaveDirectory}
                disabled={pickingDirectory || settingsSaving}
              >
                {pickingDirectory ? "选择中…" : "选择目录"}
              </button>
              <span class="meta-item subtle">新任务默认保存到这里</span>
            </div>

            <label class="settings-field">
              <span class="settings-label">登录源</span>
              <select class="settings-input" bind:value={cookieBrowser}>
                {#each browserOptions as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>

            <label class="settings-field">
              <span class="settings-label">下载模式</span>
              <select class="settings-input" bind:value={downloadMode}>
                {#each downloadModeOptions as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>

            <label class="settings-field">
              <span class="settings-label">默认清晰度策略</span>
              <select class="settings-input" bind:value={qualityPreference}>
                {#each qualityOptions as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>

            <label class="checkbox-row">
              <input bind:checked={autoRevealInFinder} type="checkbox" />
              <span>下载完成后自动在文件管理器中定位文件</span>
            </label>

            <p class="settings-help">
              当前状态：{bootstrap.accountLabel}。智能模式只会在单视频页生效，主页批量页始终会先展示可选作品。
            </p>

            <div class="settings-submit">
              <button
                class="primary-button"
                onclick={handleSaveSettings}
                disabled={settingsSaving || pickingDirectory}
              >
                {settingsSaving ? "保存中…" : "保存设置"}
              </button>
            </div>
          </div>
        </aside>
      {/if}
    </section>
  </main>
{/if}
