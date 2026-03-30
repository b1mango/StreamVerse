<script lang="ts">
  import { onMount } from "svelte";
  import {
    analyzeInput,
    clearFinishedTasks,
    createDownloadTask,
    getBootstrapState,
    listDownloadTasks,
    openInFileManager,
    pickSaveDirectory,
    saveSettings
  } from "./lib/backend";
  import type {
    AuthState,
    BootstrapState,
    DownloadMode,
    DownloadTask,
    QualityPreference,
    SettingsProfile,
    VideoAsset,
    VideoFormat
  } from "./lib/types";

  let loading = true;
  let analyzing = false;
  let downloading = false;
  let pasting = false;
  let clearingFinished = false;
  let bootstrap: BootstrapState | null = null;
  let preview: VideoAsset | null = null;
  let tasks: DownloadTask[] = [];
  let shareInput = "";
  let selectedFormatId = "";
  let errorMessage = "";
  let successMessage = "";
  let cookieBrowser = "";
  let saveDirectoryDraft = "";
  let targetDirectory = "";
  let downloadMode: DownloadMode = "manual";
  let qualityPreference: QualityPreference = "recommended";
  let autoRevealInFinder = false;
  let settingsOpen = false;
  let settingsSaving = false;
  let pickingDirectory = false;
  let pickingTargetDirectory = false;
  let openingFolder = false;
  let pollTimer: number | undefined;

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

  const qualityOptions: Array<{
    value: QualityPreference;
    label: string;
  }> = [
    { value: "recommended", label: "推荐优先" },
    { value: "highest", label: "最高质量" },
    { value: "no_watermark", label: "无水印优先" },
    { value: "smallest", label: "最小体积" }
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
      preview = bootstrap.preview;
      selectedFormatId = pickPreferredFormat(bootstrap.preview)?.id ?? "";
      shareInput = bootstrap.preview.sourceUrl;
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

  async function handleAnalyze() {
    analyzing = true;
    errorMessage = "";
    successMessage = "";

    try {
      const nextPreview = await analyzeInput({ rawInput: shareInput });
      const preferredFormat = pickPreferredFormat(nextPreview);
      preview = nextPreview;
      selectedFormatId = preferredFormat?.id ?? "";

      if (downloadMode === "smart" && preferredFormat) {
        await startDownload(nextPreview, preferredFormat, true);
      } else {
        successMessage = "解析完成，可以直接选择清晰度并开始下载。";
      }
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      analyzing = false;
    }
  }

  async function handlePasteAndAnalyze() {
    pasting = true;
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

      shareInput = text;
      await handleAnalyze();
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      pasting = false;
    }
  }

  async function handleCreateTask() {
    if (!preview || !selectedFormatId) {
      return;
    }

    const format = preview.formats.find((item) => item.id === selectedFormatId);
    if (!format) {
      return;
    }

    await startDownload(preview, format, false);
  }

  async function startDownload(
    asset: VideoAsset,
    format: VideoFormat,
    launchedBySmartMode: boolean
  ) {
    downloading = true;
    errorMessage = "";
    successMessage = "";

    try {
      const task = await createDownloadTask({
        awemeId: asset.awemeId,
        sourceUrl: asset.sourceUrl,
        title: asset.title,
        formatId: format.id,
        formatLabel: format.label,
        saveDirectoryOverride: resolvedTargetDirectory(),
        directUrl: format.directUrl ?? null,
        referer: format.referer ?? null,
        userAgent: format.userAgent ?? null
      });

      tasks = [task, ...tasks.filter((item) => item.id !== task.id)];
      successMessage = launchedBySmartMode
        ? "解析完成，已按默认策略直接开始下载。"
        : task.message ?? "下载任务已开始。";
    } catch (error) {
      errorMessage = resolveErrorMessage(error);
    } finally {
      downloading = false;
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
      if (bootstrap) {
        syncSettings(bootstrap);
      }
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

  function resolvedTargetDirectory() {
    return targetDirectory.trim() || bootstrap?.saveDirectory || "";
  }

  function pickPreferredFormat(asset: VideoAsset | null) {
    if (!asset) {
      return undefined;
    }

    const usableFormats =
      bootstrap?.authState === "active"
        ? [...asset.formats]
        : asset.formats.filter((item) => !item.requiresLogin);
    const candidateFormats =
      usableFormats.length > 0 ? usableFormats : [...asset.formats];
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

  function formatHeight(format: VideoFormat) {
    return Number.parseInt(format.resolution.split("x")[1] ?? "0", 10) || 0;
  }

  function formatDuration(totalSeconds: number) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes.toString().padStart(2, "0")}:${seconds
      .toString()
      .padStart(2, "0")}`;
  }

  function finishedTaskCount(items: DownloadTask[]) {
    return items.filter(
      (task) => task.status === "completed" || task.status === "failed"
    ).length;
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

  function selectedFormat(asset: VideoAsset | null): VideoFormat | undefined {
    return asset?.formats.find((item) => item.id === selectedFormatId);
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

      <article class="panel composer-panel">
        <div class="composer-copy">
          <p class="eyebrow">Paste Link</p>
          <h2>粘贴链接，选格式，下载到本地。</h2>
          <p class="lede">
            视觉退到后台，只保留解析、目录、清晰度和下载结果这些高频动作。
          </p>
        </div>

        <label class="input-wrap">
          <textarea
            bind:value={shareInput}
            rows="4"
            placeholder="粘贴抖音分享口令、短链或作品页链接"
          ></textarea>
        </label>

        <div class="path-strip">
          <div class="path-copy">
            <span>本次保存到</span>
            <strong title={resolvedTargetDirectory()}>{resolvedTargetDirectory()}</strong>
          </div>

          <div class="path-actions">
            <button
              class="ghost-button"
              onclick={handlePickTargetDirectory}
              disabled={pickingTargetDirectory}
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

        <div class="action-row">
          <button
            class="primary-button"
            onclick={handleAnalyze}
            disabled={analyzing || downloading || pasting}
          >
            {analyzing ? "解析中…" : "解析链接"}
          </button>

          <button
            class="secondary-button"
            onclick={handlePasteAndAnalyze}
            disabled={analyzing || downloading || pasting}
          >
            {pasting ? "读取中…" : "粘贴并解析"}
          </button>

          <button
            class="secondary-button"
            onclick={handleCreateTask}
            disabled={!preview || !selectedFormatId || analyzing || downloading || pasting}
          >
            {downloading ? "创建任务…" : "开始下载"}
          </button>
        </div>

        <div class="meta-row">
          <span class="meta-item">默认目录：{bootstrap.saveDirectory}</span>
          <span class="meta-item">
            默认策略：{qualityOptions.find((item) => item.value === qualityPreference)?.label}
          </span>
          <span class="meta-item">解析器：自动（抖音桥接 / yt-dlp）</span>
          <span class="meta-item">
            当前格式：{selectedFormat(preview)?.label ?? "等待解析"}
          </span>
        </div>

        {#if errorMessage}
          <p class="notice error">{errorMessage}</p>
        {/if}

        {#if successMessage}
          <p class="notice success">{successMessage}</p>
        {/if}
      </article>

      {#if preview}
        <section class="analysis-grid">
          <article class="panel preview-panel">
            <p class="eyebrow">Preview</p>
            <h3>{preview.title}</h3>
            <p class="preview-caption">{preview.caption}</p>

            <div class="facts">
              <div>
                <span>作者</span>
                <strong>{preview.author}</strong>
              </div>
              <div>
                <span>时长</span>
                <strong>{formatDuration(preview.durationSeconds)}</strong>
              </div>
              <div>
                <span>发布日期</span>
                <strong>{preview.publishDate}</strong>
              </div>
            </div>
          </article>

          <article class="panel formats-panel">
            <div class="section-head">
              <div>
                <p class="eyebrow">Formats</p>
                <h3>选择清晰度</h3>
              </div>
              <span class="chip subtle">{preview.formats.length} 个格式</span>
            </div>

            <div class="format-list">
              {#each preview.formats as format}
                <button
                  class:selected={selectedFormatId === format.id}
                  class="format-row"
                  onclick={() => (selectedFormatId = format.id)}
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
          </article>
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
          <p class="empty-state">还没有下载任务。先解析一个链接试试。</p>
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
                      class:failed={task.status === "failed"}
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
                  {#if task.outputPath}
                    <div class="task-actions">
                      <button
                        class="text-button"
                        onclick={() => handleRevealTask(task)}
                        type="button"
                      >
                        定位文件
                      </button>
                    </div>
                  {/if}
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
                class="settings-input"
                bind:value={saveDirectoryDraft}
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
              <span class="meta-item subtle">
                新任务默认保存到这里
              </span>
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
              当前状态：{bootstrap.accountLabel}。智能模式会在解析完成后直接按默认策略创建下载任务。
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
