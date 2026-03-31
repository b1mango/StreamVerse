<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type {
    AnalysisProgress,
    AuthState,
    DownloadContentSelection,
    QualityPreference,
    VideoAsset,
    VideoFormat
  } from "../types";
  import {
    formatDuration,
    hasSelectedDownloadOptions,
    pickPreferredFormat,
    selectedFormat,
    summarizeDownloadOptions,
    visibleFormats
  } from "../media";

  export let platformLabel = "";
  export let heroEyebrow = "Single Video";
  export let heading = "";
  export let description = "";
  export let placeholder = "";
  export let parserLabel = "";
  export let formatNote = "";
  export let inputValue = "";
  export let preview: VideoAsset | null = null;
  export let selectedFormatId = "";
  export let downloadOptions: DownloadContentSelection;
  export let authState: AuthState = "guest";
  export let qualityPreference: QualityPreference = "recommended";
  export let qualityLabel = "";
  export let analyzing = false;
  export let analysisProgress: AnalysisProgress | null = null;
  export let downloading = false;
  export let pasting = false;

  const dispatch = createEventDispatcher<{
    analyze: void;
    paste: void;
    download: void;
  }>();

  $: formatList = visibleFormats(preview, authState);
  $: currentFormat = selectedFormat(preview, selectedFormatId, authState);
  $: preferredFormat = pickPreferredFormat(preview, qualityPreference, authState);
  $: analysisPercent = analysisProgress
    ? Math.max(6, Math.min(100, Math.round((analysisProgress.current / Math.max(analysisProgress.total, 1)) * 100)))
    : 0;
</script>

<section class="page-shell">
  <article class="panel page-hero">
    <div class="composer-copy">
      <p class="eyebrow">{heroEyebrow}</p>
      <h2>{heading}</h2>
      {#if description}
        <p class="lede">{description}</p>
      {/if}
    </div>

    <label class="input-wrap">
      <textarea bind:value={inputValue} rows="4" placeholder={placeholder}></textarea>
    </label>

    <div class="selection-block">
      <div class="section-head compact">
        <div>
          <p class="eyebrow">Download Items</p>
          <h3>下载内容</h3>
        </div>
        <span class="chip subtle">
          {hasSelectedDownloadOptions(downloadOptions)
            ? summarizeDownloadOptions(downloadOptions)
            : "未选择"}
        </span>
      </div>

      <div class="option-grid">
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadVideo} type="checkbox" />
          <span>视频</span>
        </label>
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadCover} type="checkbox" />
          <span>封面</span>
        </label>
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadCaption} type="checkbox" />
          <span>文案</span>
        </label>
        <label class="option-chip subtle-option">
          <input bind:checked={downloadOptions.downloadMetadata} type="checkbox" />
          <span>元数据</span>
        </label>
      </div>
    </div>

    <div class="action-row">
      <button
        class="primary-button"
        onclick={() => dispatch("analyze")}
        disabled={analyzing || downloading || pasting}
      >
        {analyzing ? "解析中…" : "解析作品"}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("paste")}
        disabled={analyzing || downloading || pasting}
      >
        {pasting ? "读取中…" : "粘贴并解析"}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("download")}
        disabled={!preview || analyzing || downloading}
      >
        {downloading ? "创建任务…" : "开始下载"}
      </button>
    </div>

    {#if analyzing && analysisProgress}
      <div class="analysis-progress-card">
        <div class="section-head compact">
          <div>
            <p class="eyebrow">Analyze Progress</p>
            <h3>解析进度</h3>
          </div>
          <span class="chip subtle">
            {analysisProgress.current} / {analysisProgress.total}
          </span>
        </div>
        <div class="task-progress analysis-progress-bar">
          <div class="task-progress-fill" style={`width: ${analysisPercent}%`}></div>
        </div>
        <p class="analysis-progress-copy">{analysisProgress.message}</p>
      </div>
    {/if}

    <div class="meta-row">
      <span class="meta-item">平台：{platformLabel}</span>
      <span class="meta-item">默认策略：{qualityLabel}</span>
      <span class="meta-item">当前格式：{currentFormat?.label ?? preferredFormat?.label ?? "等待解析"}</span>
      {#if parserLabel}
        <span class="meta-item">{parserLabel}</span>
      {/if}
    </div>

    {#if formatNote}
      <div class="meta-row page-meta">
        <span class="meta-item">{formatNote}</span>
      </div>
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
          <span class="chip subtle">{formatList.length} 个格式</span>
        </div>

        {#if downloadOptions.downloadVideo}
          <div class="format-list">
            {#each formatList as format}
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
          <p class="empty-state">当前只保存附加内容，所以这里不需要选择清晰度。</p>
        {/if}
      </article>
    </section>
  {/if}
</section>
