<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type {
    AnalysisProgress,
    AuthState,
    DownloadContentSelection,
    ProfileBatch,
    VideoAsset
  } from "../types";
  import {
    formatDuration,
    hasSelectedDownloadOptions,
    pickPreferredFormat,
    selectedFormat,
    summarizeDownloadOptions
  } from "../media";
  import { visibleFormats } from "../media";

  export let inputValue = "";
  export let preview: ProfileBatch | null = null;
  export let selectedIds: string[] = [];
  export let selectedFormatIdsByAssetId: Record<string, string> = {};
  export let downloadOptions: DownloadContentSelection;
  export let authState: AuthState = "guest";
  export let heroEyebrow = "Profile Batch";
  export let heading = "主页批量下载";
  export let description = "";
  export let placeholder = "粘贴主页分享文案或主页链接";
  export let analyzeLabel = "浏览器读取列表";
  export let analyzeLoadingLabel = "读取主页…";
  export let pasteLabel = "粘贴并读取";
  export let pasteLoadingLabel = "读取中…";
  export let enqueueLabel = "将所选作品加入队列";
  export let enqueuingLabel = "加入队列中…";
  export let itemLabel = "作品";
  export let resultEyebrow = "Profile Result";
  export let showPrepareAction = false;
  export let prepareLabel = "打开浏览器";
  export let prepareLoadingLabel = "打开中…";
  export let preparing = false;
  export let analyzeDisabled = false;
  export let analyzing = false;
  export let analysisProgress: AnalysisProgress | null = null;
  export let enqueuing = false;
  export let pasting = false;
  let filterText = "";
  let lastToggledIndex: number | null = null;

  const dispatch = createEventDispatcher<{
    prepare: void;
    analyze: void;
    paste: void;
    enqueue: void;
    selectionChange: { ids: string[] };
    formatChange: { assetId: string; formatId: string };
    selectAll: void;
    clearSelection: void;
    close: void;
  }>();

  function isSelected(assetId: string) {
    return selectedIds.includes(assetId);
  }

  function orderedSelection(ids: Set<string>) {
    if (!preview) {
      return Array.from(ids);
    }

    return preview.items
      .map((item) => item.assetId)
      .filter((assetId) => ids.has(assetId));
  }

  function syncSelectedIds(next: string[]) {
    selectedIds = next;
    dispatch("selectionChange", { ids: next });
  }

  function toggleSelection(event: Event, assetId: string, index: number) {
    const target = event.currentTarget as HTMLInputElement | null;
    if (!target) {
      return;
    }

    const checked = target.checked;
    const ids = new Set(selectedIds);
    const mouseEvent = event as MouseEvent;

    if (mouseEvent.shiftKey && lastToggledIndex !== null) {
      const start = Math.min(lastToggledIndex, index);
      const end = Math.max(lastToggledIndex, index);
      for (const item of filteredItems.slice(start, end + 1)) {
        if (checked) {
          ids.add(item.assetId);
        } else {
          ids.delete(item.assetId);
        }
      }
    } else if (checked) {
      ids.add(assetId);
    } else {
      ids.delete(assetId);
    }

    syncSelectedIds(orderedSelection(ids));
    lastToggledIndex = index;
  }

  function selectAllItems() {
    syncSelectedIds(preview?.items.map((item) => item.assetId) ?? []);
    lastToggledIndex = null;
    dispatch("selectAll");
  }

  function clearSelectedItems() {
    syncSelectedIds([]);
    lastToggledIndex = null;
    dispatch("clearSelection");
  }

  function formatsForItem(item: VideoAsset) {
    return visibleFormats(item, authState);
  }

  function currentFormatId(item: VideoAsset) {
    return (
      selectedFormatIdsByAssetId[item.assetId] ??
      pickPreferredFormat(item, "recommended", authState)?.id ??
      ""
    );
  }

  function currentFormatLabel(item: VideoAsset) {
    return (
      selectedFormat(item, currentFormatId(item), authState)?.label ??
      pickPreferredFormat(item, "recommended", authState)?.label ??
      "等待选择"
    );
  }

  function hasFormats(item: VideoAsset) {
    return formatsForItem(item).length > 0;
  }

  function normalized(value: string) {
    return value.trim().toLowerCase();
  }

  function itemMetaLine(item: VideoAsset) {
    const parts: string[] = [];

    if (item.categoryLabel) {
      parts.push(item.categoryLabel);
    }

    if (item.groupTitle) {
      parts.push(item.groupTitle);
    }

    if (item.publishDate && item.publishDate !== "未知") {
      parts.push(item.publishDate);
    }

    if (item.durationSeconds > 0) {
      parts.push(formatDuration(item.durationSeconds));
    }

    return parts.join(" · ") || "未知";
  }

  function summarizeCategories(items: VideoAsset[]) {
    const counts = new Map<string, number>();

    for (const item of items) {
      const label = item.categoryLabel?.trim();
      if (!label) {
        continue;
      }
      counts.set(label, (counts.get(label) ?? 0) + 1);
    }

    return Array.from(counts.entries()).map(([label, count]) => `${label} ${count}`);
  }

  $: filteredItems = preview
    ? preview.items.filter((item) => {
        const query = normalized(filterText);
        if (!query) {
          return true;
        }

        return [
          item.title,
          item.author,
          item.publishDate,
          item.sourceUrl,
          item.categoryLabel,
          item.groupTitle
        ]
          .join(" ")
          .toLowerCase()
          .includes(query);
      })
    : [];
  $: categoryStats = preview ? summarizeCategories(preview.items) : [];
  $: analysisPercent = analysisProgress
    ? Math.max(6, Math.min(100, Math.round((analysisProgress.current / Math.max(analysisProgress.total, 1)) * 100)))
    : 0;
  $: selectedIdSet = new Set(selectedIds);
  $: analysisCounterLabel = analysisProgress
    ? `已读取 ${analysisProgress.current} / ${analysisProgress.total} 个${itemLabel}`
    : "";
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
      <textarea bind:value={inputValue} rows="4" {placeholder}></textarea>
    </label>

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
          <span>元数据</span>
        </label>
      </div>
    </div>

    <div class="action-row">
      {#if showPrepareAction}
        <button
          class="primary-button"
          onclick={() => dispatch("prepare")}
          disabled={preparing || analyzing || enqueuing || pasting}
        >
          {preparing ? prepareLoadingLabel : prepareLabel}
        </button>
      {/if}
      <button
        class={showPrepareAction ? "secondary-button" : "primary-button"}
        onclick={() => dispatch("analyze")}
        disabled={preparing || analyzing || enqueuing || pasting || analyzeDisabled}
      >
        {analyzing ? analyzeLoadingLabel : analyzeLabel}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("paste")}
        disabled={preparing || analyzing || enqueuing || pasting}
      >
        {pasting ? pasteLoadingLabel : pasteLabel}
      </button>
      <button
        class="secondary-button"
        onclick={() => dispatch("enqueue")}
        disabled={!preview || enqueuing || !selectedIds.length}
      >
        {enqueuing ? enqueuingLabel : enqueueLabel}
      </button>
    </div>

  </article>

  {#if analyzing && analysisProgress}
    <article class="panel profile-progress-panel">
      <div class="section-head compact">
        <div>
          <p class="eyebrow">Analyze Progress</p>
          <h3>{analysisCounterLabel}</h3>
        </div>
        <span class="chip subtle">{analysisPercent}%</span>
      </div>
      <div class="task-progress analysis-progress-bar">
        <div class="task-progress-fill" style={`width: ${analysisPercent}%`}></div>
      </div>
      <p class="analysis-progress-copy">{analysisProgress.message}</p>
    </article>
  {/if}

  {#if preview}
    <article class="panel profile-panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">{resultEyebrow}</p>
          <h3>{preview.profileTitle}</h3>
        </div>

        <div class="section-actions">
          <span class="chip subtle">已选 {selectedIds.length} / {preview.items.length} 个{itemLabel}</span>
          <button class="text-button" onclick={selectAllItems} type="button">
            全选
          </button>
          <button class="text-button" onclick={clearSelectedItems} type="button">
            清空
          </button>
          <button class="text-button" onclick={() => dispatch("close")} type="button">
            关闭
          </button>
        </div>
      </div>

      <div class="profile-toolbar">
        <div class="meta-row">
          <span class="meta-item">已读取 {preview.fetchedCount} 个{itemLabel}</span>
          <span class="meta-item">已选 {selectedIds.length} 个</span>
          {#each categoryStats as stat}
            <span class="meta-item subtle">{stat}</span>
          {/each}
        </div>

        <label class="search-field">
          <input
            bind:value={filterText}
            class="settings-input"
            placeholder={`筛选${itemLabel}标题`}
            type="text"
          />
        </label>
      </div>

      <div class="profile-list">
        {#if filteredItems.length === 0}
          <p class="empty-state">没有匹配的{itemLabel}</p>
        {:else}
          {#each filteredItems as item, index (item.assetId)}
            {@const selected = selectedIdSet.has(item.assetId)}
            <div class:selected-row={selected} class="profile-row">
              <div class="profile-check">
                {#key `${item.assetId}:${selected ? "1" : "0"}`}
                  <input
                    checked={selected}
                    onchange={(event) => toggleSelection(event, item.assetId, index)}
                    type="checkbox"
                  />
                {/key}
              </div>

              <div class="profile-copy">
                <strong>{item.title}</strong>
                <span>{itemMetaLine(item)}</span>
              </div>

              {#if downloadOptions.downloadVideo && hasFormats(item)}
                <div class="profile-format-slot">
                  <small>下载清晰度：{currentFormatLabel(item)}</small>
                  <select
                    class="compact-select"
                    disabled={!selected}
                    value={currentFormatId(item)}
                    onchange={(event) =>
                      dispatch("formatChange", {
                        assetId: item.assetId,
                        formatId: (event.currentTarget as HTMLSelectElement).value
                      })}
                  >
                    {#each formatsForItem(item) as format}
                      <option value={format.id}>
                        {format.label} · {format.codec} · {format.container}
                      </option>
                    {/each}
                  </select>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </article>
  {/if}
</section>
