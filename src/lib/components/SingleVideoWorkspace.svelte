<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { t } from "../i18n";
  import { fetchThumbnail } from "../backend";
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
    formatFileSize,
    estimateFileSize,
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

  let proxiedCoverUrl: string | null = null;
  let lastCoverUrl: string | null = null;

  $: if (preview?.coverUrl && preview.coverUrl !== lastCoverUrl) {
    lastCoverUrl = preview.coverUrl;
    proxiedCoverUrl = null;
    fetchThumbnail(preview.coverUrl).then((dataUri) => {
      if (preview?.coverUrl === lastCoverUrl) {
        proxiedCoverUrl = dataUri;
      }
    }).catch(() => {});
  } else if (!preview?.coverUrl) {
    lastCoverUrl = null;
    proxiedCoverUrl = null;
  }

  function formatSizeLabel(format: import("../types").VideoFormat): string {
    const bytes = format.fileSizeBytes
      ?? estimateFileSize(format.bitrateKbps, preview?.durationSeconds ?? 0);
    if (!bytes) return "";
    const prefix = format.fileSizeBytes ? "" : "≈";
    return `${prefix}${formatFileSize(bytes)}`;
  }
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
          <h3>{$t("single.downloadContent")}</h3>
        </div>
        <span class="chip subtle">
          {hasSelectedDownloadOptions(downloadOptions)
            ? summarizeDownloadOptions(downloadOptions)
            : $t("common.notSelected")}
        </span>
      </div>

      <div class="option-grid">
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadVideo} type="checkbox" />
          <span>{$t("content.video")}</span>
        </label>
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadCover} type="checkbox" />
          <span>{$t("content.cover")}</span>
        </label>
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadCaption} type="checkbox" />
          <span>{$t("content.caption")}</span>
        </label>
        <label class="option-chip subtle-option">
          <input bind:checked={downloadOptions.downloadMetadata} type="checkbox" />
          <span>{$t("content.metadata")}</span>
        </label>
      </div>
    </div>

    <div class="action-row">
      <button
        class="primary-button"
        onclick={() => dispatch("analyze")}
        disabled={analyzing || downloading || pasting}
      >
        {analyzing ? $t("common.analyzing") : $t("single.analyze")}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("download")}
        disabled={!preview || analyzing || downloading}
      >
        {downloading ? $t("single.creatingTask") : $t("single.startDownload")}
      </button>
    </div>

    {#if analyzing && analysisProgress}
      <div class="analysis-progress-card">
        <div class="section-head compact">
          <div>
            <p class="eyebrow">Analyze Progress</p>
            <h3>{$t("single.analyzeProgress")}</h3>
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
      <span class="meta-item">{$t("single.platform")}：{platformLabel}</span>
      <span class="meta-item">{$t("single.defaultStrategy")}：{qualityLabel}</span>
      <span class="meta-item">{$t("single.currentFormat")}：{currentFormat?.label ?? preferredFormat?.label ?? $t("single.awaitingAnalysis")}</span>
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
        {#if proxiedCoverUrl}
          <img
            src={proxiedCoverUrl}
            alt={preview.title}
            class="preview-thumbnail"
          />
        {/if}
        <h3>{preview.title}</h3>
        <p class="preview-caption">{preview.caption}</p>

        <div class="facts">
          <div>
            <span>{$t("single.author")}</span>
            <strong>{preview.author}</strong>
          </div>
          <div>
            <span>{$t("single.duration")}</span>
            <strong>{formatDuration(preview.durationSeconds)}</strong>
          </div>
          <div>
            <span>{$t("single.publishDate")}</span>
            <strong>{preview.publishDate}</strong>
          </div>
        </div>
      </article>

      <article class="panel formats-panel">
        <div class="section-head">
          <div>
            <p class="eyebrow">Formats</p>
            <h3>{$t("single.selectQuality")}</h3>
          </div>
          <span class="chip subtle">{formatList.length} {$t("single.formatsCount")}</span>
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
                    {#if formatSizeLabel(format)}
                      · {formatSizeLabel(format)}
                    {/if}
                  </span>
                </div>

                <div class="format-tags">
                  {#if format.recommended}
                    <span class="mini-tag accent">{$t("format.recommended")}</span>
                  {/if}
                  {#if format.noWatermark}
                    <span class="mini-tag">{$t("format.noWatermark")}</span>
                  {/if}
                  {#if format.requiresLogin}
                    <span class="mini-tag">{$t("format.requiresLogin")}</span>
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        {:else}
          <p class="empty-state">{$t("single.noVideoSelected")}</p>
        {/if}
      </article>
    </section>
  {/if}
</section>
