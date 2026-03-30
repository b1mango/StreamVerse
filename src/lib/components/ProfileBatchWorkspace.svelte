<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { DownloadContentSelection, ProfileBatch } from "../types";
  import {
    clampBatchLimit,
    formatDuration,
    hasSelectedDownloadOptions,
    summarizeDownloadOptions
  } from "../media";

  export let inputValue = "";
  export let preview: ProfileBatch | null = null;
  export let selectedIds: string[] = [];
  export let batchLimit = 24;
  export let downloadOptions: DownloadContentSelection;
  export let analyzing = false;
  export let enqueuing = false;
  export let pasting = false;
  export let qualityLabel = "";

  const dispatch = createEventDispatcher<{
    analyze: void;
    paste: void;
    enqueue: void;
    toggle: { assetId: string; checked: boolean };
    selectAll: void;
    clearSelection: void;
    close: void;
  }>();

  function isSelected(assetId: string) {
    return selectedIds.includes(assetId);
  }
</script>

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
        bind:value={inputValue}
        rows="4"
        placeholder="粘贴抖音个人主页分享文案或主页链接"
      ></textarea>
    </label>

    <div class="profile-topline">
      <label class="inline-count-field">
        <span>主页批量上限</span>
        <input
          bind:value={batchLimit}
          class="inline-count-input"
          max="100"
          min="1"
          onchange={() => (batchLimit = clampBatchLimit(batchLimit))}
          type="number"
        />
      </label>

      <div class="meta-row page-meta">
        <span class="meta-item">批量会按“{qualityLabel}”自动选格式</span>
      </div>
    </div>

    <div class="selection-block">
      <div class="section-head compact">
        <div>
          <p class="eyebrow">Batch Items</p>
          <h3>批量下载内容</h3>
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
        onclick={() => dispatch("analyze")}
        disabled={analyzing || enqueuing || pasting}
      >
        {analyzing ? "读取主页…" : "读取主页作品"}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("paste")}
        disabled={analyzing || enqueuing || pasting}
      >
        {pasting ? "读取中…" : "粘贴并读取"}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("enqueue")}
        disabled={!preview || enqueuing || !selectedIds.length}
      >
        {enqueuing ? "加入队列中…" : "将所选作品加入队列"}
      </button>
    </div>
  </article>

  {#if preview}
    <article class="panel profile-panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Profile Result</p>
          <h3>{preview.profileTitle}</h3>
        </div>

        <div class="section-actions">
          <span class="chip subtle">已选 {selectedIds.length} / {preview.items.length}</span>
          <button class="text-button" onclick={() => dispatch("selectAll")} type="button">
            全选
          </button>
          <button class="text-button" onclick={() => dispatch("clearSelection")} type="button">
            清空
          </button>
          <button class="text-button" onclick={() => dispatch("close")} type="button">
            关闭
          </button>
        </div>
      </div>

      <div class="meta-row">
        <span class="meta-item">本次读取 {preview.fetchedCount} 个作品</span>
        <span class="meta-item">勾选后的作品会统一使用当前批量下载内容设置</span>
      </div>

      <div class="profile-list">
        {#each preview.items as item}
          <label class="profile-row">
            <input
              checked={isSelected(item.assetId)}
              onchange={(event) =>
                dispatch("toggle", {
                  assetId: item.assetId,
                  checked: (event.currentTarget as HTMLInputElement).checked
                })}
              type="checkbox"
            />

            <div class="profile-copy">
              <strong>{item.title}</strong>
              <span>{item.publishDate} · {formatDuration(item.durationSeconds)}</span>
            </div>
          </label>
        {/each}
      </div>
    </article>
  {/if}
</section>
