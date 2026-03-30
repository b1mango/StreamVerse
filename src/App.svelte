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
    downloadModeOptions,
    modeLabelMap,
    platformMeta,
    qualityOptions
  } from "./lib/options";
  import type {
    BootstrapState,
    DownloadContentSelection,
    DownloadMode,
    DownloadTask,
    PlatformId,
    ProfileBatch,
    QualityPreference,
    SettingsProfile,
    VideoAsset,
    VideoFormat
  } from "./lib/types";

  type DouyinPage = "single" | "profile";

  let loading = true;
  let settingsOpen = false;
  let settingsSaving = false;
  let pickingDirectory = false;
  let pickingTargetDirectory = false;
  let openingFolder = false;
  let clearingFinished = false;
  let activePlatform: PlatformId | null = null;
  let activeDouyinPage: DouyinPage = "single";
  let bootstrap: BootstrapState | null = null;
  let tasks: DownloadTask[] = [];
  let pollTimer: number | undefined;

  let douyinSingleInput = "";
  let douyinSinglePreview: VideoAsset | null = null;
  let douyinSelectedFormatId = "";
  let douyinSingleOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingDouyinSingle = false;
  let downloadingDouyinSingle = false;
  let pastingDouyinSingle = false;

  let douyinProfileInput = "";
  let douyinProfilePreview: ProfileBatch | null = null;
  let douyinSelectedProfileIds: string[] = [];
  let douyinProfileLimit = 24;
  let douyinProfileOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingDouyinProfile = false;
  let enqueuingDouyinProfile = false;
  let pastingDouyinProfile = false;

  let bilibiliInput = "";
  let bilibiliPreview: VideoAsset | null = null;
  let bilibiliSelectedFormatId = "";
  let bilibiliOptions: DownloadContentSelection = createDefaultDownloadOptions();
  let analyzingBilibili = false;
  let downloadingBilibili = false;
  let pastingBilibili = false;

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
        tasks = await listDownloadTasks();
      }, 1200);
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

  function openPlatform(platform: PlatformId) {
    activePlatform = platform;
    clearNotices();
  }

  function backToPlatformHome() {
    activePlatform = null;
    clearNotices();
  }

  function switchDouyinPage(page: DouyinPage) {
    activeDouyinPage = page;
    clearNotices();
  }

  async function analyzeSinglePreview(
    platform: PlatformId,
    rawInput: string
  ): Promise<VideoAsset> {
    const preview = await analyzeInput({ rawInput });
    if (preview.platform !== platform) {
      throw new Error(
        `当前是 ${platformMeta[platform].label} 工作区，请粘贴对应平台的链接。`
      );
    }

    return preview;
  }

  async function handleAnalyzeDouyinSingle() {
    analyzingDouyinSingle = true;
    clearNotices();

    try {
      const preview = await analyzeSinglePreview("douyin", douyinSingleInput);
      douyinSinglePreview = preview;
      douyinSelectedFormatId =
        pickPreferredFormat(preview, qualityPreference, bootstrap!.authState)?.id ?? "";

      if (downloadMode === "smart" && douyinSingleOptions.downloadVideo) {
        await startSingleDownload("douyin", preview, douyinSelectedFormatId, douyinSingleOptions, true);
        return;
      }

      successMessage = "抖音作品解析完成，可以开始单条下载。";
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
      const preview = await analyzeSinglePreview("bilibili", bilibiliInput);
      bilibiliPreview = preview;
      bilibiliSelectedFormatId =
        pickPreferredFormat(preview, qualityPreference, bootstrap!.authState)?.id ?? "";

      if (downloadMode === "smart" && bilibiliOptions.downloadVideo) {
        await startSingleDownload(
          "bilibili",
          preview,
          bilibiliSelectedFormatId,
          bilibiliOptions,
          true
        );
        return;
      }

      successMessage = "Bilibili 视频解析完成，可以开始下载。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingBilibili = false;
    }
  }

  async function handleAnalyzeDouyinProfile() {
    analyzingDouyinProfile = true;
    clearNotices();

    try {
      const result = await analyzeProfileInput({
        rawInput: douyinProfileInput,
        limit: clampBatchLimit(douyinProfileLimit)
      });
      douyinProfilePreview = result;
      douyinSelectedProfileIds = result.items.map((item) => item.assetId);
      successMessage = "主页作品已载入，请勾选后再加入下载队列。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzingDouyinProfile = false;
    }
  }

  async function pasteAndAnalyze(platform: PlatformId, kind: "single" | "profile") {
    if (platform === "douyin" && kind === "single") {
      pastingDouyinSingle = true;
    } else if (platform === "douyin") {
      pastingDouyinProfile = true;
    } else {
      pastingBilibili = true;
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
      } else {
        bilibiliInput = text;
        await handleAnalyzeBilibiliSingle();
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      if (platform === "douyin" && kind === "single") {
        pastingDouyinSingle = false;
      } else if (platform === "douyin") {
        pastingDouyinProfile = false;
      } else {
        pastingBilibili = false;
      }
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
      } else {
        downloadingBilibili = value;
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
        userAgent: format?.userAgent ?? null
      });

      upsertTask(task);
      successMessage = launchedBySmartMode
        ? `${platformMeta[platform].label} 作品解析完成，已按默认策略直接创建下载任务。`
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
        items,
        saveDirectoryOverride: resolvedTargetDirectory(),
        downloadOptions: douyinProfileOptions
      });
      tasks = await listDownloadTasks();
      successMessage = result.message;
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      enqueuingDouyinProfile = false;
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

  function upsertTask(task: DownloadTask) {
    tasks = [task, ...tasks.filter((item) => item.id !== task.id)];
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

  function closeProfileSelection() {
    douyinProfilePreview = null;
    douyinSelectedProfileIds = [];
  }

  function toggleProfileSelection(assetId: string, checked: boolean) {
    douyinSelectedProfileIds = checked
      ? Array.from(new Set([...douyinSelectedProfileIds, assetId]))
      : douyinSelectedProfileIds.filter((id) => id !== assetId);
  }

  function currentQualityLabel() {
    return (
      qualityOptions.find((item) => item.value === qualityPreference)?.label ?? "推荐优先"
    );
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
          <h1>{activePlatform ? `${platformMeta[activePlatform].label} 工作区` : "选择下载平台"}</h1>
          <p class="status-copy">
            {authMap[bootstrap.authState]} · {modeLabelMap[bootstrap.downloadMode]}
          </p>
        </div>

        <div class="topbar-actions">
          {#if activePlatform}
            <button class="ghost-button" onclick={backToPlatformHome}>返回平台首页</button>
          {/if}
          <button class="ghost-button" onclick={handleOpenSettings}>设置</button>
        </div>
      </header>

      {#if activePlatform}
        <div class="workspace-switch">
          {#each Object.entries(platformMeta) as [platform, meta]}
            <button
              class:active={activePlatform === platform}
              class="workspace-tab"
              onclick={() => openPlatform(platform as PlatformId)}
              type="button"
            >
              <strong>{meta.label}</strong>
              <span>{meta.status}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if activePlatform}
        <SharedDirectoryBar
          currentDirectory={resolvedTargetDirectory()}
          defaultDirectory={bootstrap.saveDirectory}
          disabled={
            pickingTargetDirectory ||
            enqueuingDouyinProfile ||
            downloadingDouyinSingle ||
            downloadingBilibili ||
            openingFolder
          }
          picking={pickingTargetDirectory}
          on:open={handleOpenCurrentDirectory}
          on:pick={handlePickTargetDirectory}
          on:reset={() => (targetDirectory = bootstrap!.saveDirectory)}
        />
      {/if}

      {#if errorMessage}
        <p class="notice error">{errorMessage}</p>
      {/if}

      {#if successMessage}
        <p class="notice success">{successMessage}</p>
      {/if}

      {#if !activePlatform}
        <PlatformHome on:select={(event) => openPlatform(event.detail.platform)} />
      {:else if activePlatform === "douyin"}
        <section class="platform-workspace">
          <div class="mode-switch">
            <button
              class:active={activeDouyinPage === "single"}
              class="mode-pill"
              onclick={() => switchDouyinPage("single")}
              type="button"
            >
              单视频下载
            </button>
            <button
              class:active={activeDouyinPage === "profile"}
              class="mode-pill"
              onclick={() => switchDouyinPage("profile")}
              type="button"
            >
              主页批量下载
            </button>
          </div>

          {#if activeDouyinPage === "single"}
            <SingleVideoWorkspace
              authState={bootstrap.authState}
              bind:downloadOptions={douyinSingleOptions}
              bind:inputValue={douyinSingleInput}
              bind:selectedFormatId={douyinSelectedFormatId}
              description="单视频页只关注一条抖音链接：解析作品、选择下载内容、确认清晰度，然后立即开始下载。"
              downloading={downloadingDouyinSingle}
              ffmpegAvailable={bootstrap.ffmpegAvailable}
              formatNote="抖音格式优先使用直链下载；登录后通常能拿到更完整的清晰度。"
              heading="抖音单视频下载"
              parserLabel="自动（抖音桥接 / yt-dlp）"
              pasting={pastingDouyinSingle}
              platformLabel="抖音"
              placeholder="粘贴抖音分享文案、短链或作品页链接"
              preview={douyinSinglePreview}
              qualityLabel={currentQualityLabel()}
              qualityPreference={qualityPreference}
              analyzing={analyzingDouyinSingle}
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
          {:else}
            <ProfileBatchWorkspace
              bind:batchLimit={douyinProfileLimit}
              bind:downloadOptions={douyinProfileOptions}
              bind:inputValue={douyinProfileInput}
              bind:selectedIds={douyinSelectedProfileIds}
              analyzing={analyzingDouyinProfile}
              enqueuing={enqueuingDouyinProfile}
              pasting={pastingDouyinProfile}
              preview={douyinProfilePreview}
              qualityLabel={currentQualityLabel()}
              on:analyze={handleAnalyzeDouyinProfile}
              on:clearSelection={clearProfileSelection}
              on:close={closeProfileSelection}
              on:enqueue={handleEnqueueDouyinProfileTasks}
              on:paste={() => pasteAndAnalyze("douyin", "profile")}
              on:selectAll={selectAllProfileItems}
              on:toggle={(event) =>
                toggleProfileSelection(event.detail.assetId, event.detail.checked)}
            />
          {/if}
        </section>
      {:else if activePlatform === "bilibili"}
        <section class="platform-workspace">
          <div class="mode-switch">
            <button class="mode-pill active" type="button">单视频下载</button>
            <button class="mode-pill is-disabled" disabled type="button">
              UP 主批量下载
            </button>
          </div>

          <SingleVideoWorkspace
            authState={bootstrap.authState}
            bind:downloadOptions={bilibiliOptions}
            bind:inputValue={bilibiliInput}
            bind:selectedFormatId={bilibiliSelectedFormatId}
            description="Bilibili 页面对标成熟下载器的单视频流程：先解析 BV/av 链接，再选择内容与清晰度。高质量 DASH 格式需要 FFmpeg 合并音视频。"
            downloading={downloadingBilibili}
            ffmpegAvailable={bootstrap.ffmpegAvailable}
            formatNote="Bilibili 的高质量清晰度通常是分离音视频流；未安装 FFmpeg 时，建议先安装后再下载高清格式。"
            heading="Bilibili 单视频下载"
            parserLabel="自动（yt-dlp / 浏览器 Cookie）"
            pasting={pastingBilibili}
            platformLabel="Bilibili"
            placeholder="粘贴 Bilibili 视频链接、分享文案、BV 号或 b23.tv 短链"
            preview={bilibiliPreview}
            qualityLabel={currentQualityLabel()}
            qualityPreference={qualityPreference}
            analyzing={analyzingBilibili}
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
      {:else}
        <article class="panel placeholder-panel">
          <p class="eyebrow">YouTube</p>
          <h2>YouTube 工作区即将接入</h2>
          <p class="lede">
            平台入口和页面结构已经预留好，等 Bilibili 单视频链路稳定后，会继续接入 YouTube。
          </p>
        </article>
      {/if}

      <TaskQueuePanel
        tasks={tasks}
        pendingTaskIds={taskActionPendingIds}
        {clearingFinished}
        on:cancel={(event) => handleTaskControl(event.detail.task, "cancel")}
        on:clearFinished={handleClearFinished}
        on:pause={(event) => handleTaskControl(event.detail.task, "pause")}
        on:resume={(event) => handleTaskControl(event.detail.task, "resume")}
        on:reveal={(event) => handleRevealTask(event.detail.task)}
      />

      <SettingsPanel
        open={settingsOpen}
        bind:autoRevealInFinder
        bind:cookieBrowser
        bind:downloadMode
        bind:qualityPreference
        bind:saveDirectoryDraft
        accountLabel={bootstrap.accountLabel}
        browserOptions={browserOptions}
        downloadModeOptions={downloadModeOptions}
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
