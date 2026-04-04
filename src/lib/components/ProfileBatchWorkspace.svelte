<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { t, tRaw } from "../i18n";
  import { fetchThumbnail } from "../backend";
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
  export let downloadedAssetIds: string[] = [];
  export let authState: AuthState = "guest";
  export let heroEyebrow = "Profile Batch";
  export let heading = "主页批量下载";
  export let description = "";
  export let placeholder = "粘贴主页分享文案或主页链接";
  export let analyzeLabel = "浏览器读取列表";
  export let analyzeLoadingLabel = "读取主页…";
  export let enqueueLabel = "下载所选作品";
  export let enqueuingLabel = "下载中…";
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

  let thumbnailCache: Record<string, string> = {};
  let thumbnailLastPreviewId = "";
  let thumbnailAbort: (() => void) | null = null;

  $: if (preview && preview.items.length > 0) {
    const previewId = preview.items.map(i => i.assetId).join(",");
    if (previewId !== thumbnailLastPreviewId) {
      thumbnailLastPreviewId = previewId;
      thumbnailCache = {};
      if (thumbnailAbort) { thumbnailAbort(); thumbnailAbort = null; }

      let cancelled = false;
      thumbnailAbort = () => { cancelled = true; };

      const queue = preview.items
        .filter(item => item.coverUrl)
        .map(item => ({ aid: item.assetId, url: item.coverUrl! }));

      (async () => {
        const batch = 6;
        for (let i = 0; i < queue.length; i += batch) {
          if (cancelled) return;
          const chunk = queue.slice(i, i + batch);
          await Promise.allSettled(
            chunk.map(({ aid, url }) =>
              fetchThumbnail(url).then((dataUri) => {
                if (!cancelled) {
                  thumbnailCache[aid] = dataUri;
                  thumbnailCache = thumbnailCache;
                }
              })
            )
          );
        }
      })();
    }
  }

  const dispatch = createEventDispatcher<{
    prepare: void;
    analyze: void;
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

  let lastShiftKey = false;

  function trackShiftKey(event: MouseEvent) {
    lastShiftKey = event.shiftKey;
  }

  function toggleSelection(event: Event, assetId: string, index: number) {
    const target = event.currentTarget as HTMLInputElement | null;
    if (!target) {
      return;
    }

    const checked = target.checked;
    const ids = new Set(selectedIds);

    if (lastShiftKey && lastToggledIndex !== null) {
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

  function invertSelection() {
    if (!preview) return;
    const currentSet = new Set(selectedIds);
    const inverted = preview.items
      .filter((item) => !currentSet.has(item.assetId))
      .map((item) => item.assetId);
    syncSelectedIds(inverted);
    lastToggledIndex = null;
  }

  // --- Drag-select ---
  let dragActive = false;
  let dragStartX = 0;
  let dragStartY = 0;
  let dragCurrentX = 0;
  let dragCurrentY = 0;
  let profileListEl: HTMLElement | null = null;
  let selectionBeforeDrag: Set<string> = new Set();

  function attachDragSelection(node: HTMLElement) {
    profileListEl = node;
    const handleMouseDown = (event: MouseEvent) => onDragStart(event);
    node.addEventListener("mousedown", handleMouseDown);

    return {
      destroy() {
        node.removeEventListener("mousedown", handleMouseDown);
      }
    };
  }

  function stopDragSelection() {
    dragActive = false;
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);
    window.removeEventListener("blur", stopDragSelection);
    window.removeEventListener("contextmenu", stopDragSelection);
    document.removeEventListener("visibilitychange", handleVisibilityChange);
  }

  function handleVisibilityChange() {
    if (document.visibilityState !== "visible") {
      stopDragSelection();
    }
  }

  function onDragStart(event: MouseEvent) {
    // Only start drag on left button with a modifier key in empty space.
    const target = event.target as HTMLElement;
    if (target.closest("input, button, select, a, .profile-format-slot")) return;
    if (event.button !== 0) return;
    if (!(event.shiftKey || event.altKey)) return;

    dragActive = true;
    dragStartX = event.clientX;
    dragStartY = event.clientY;
    dragCurrentX = event.clientX;
    dragCurrentY = event.clientY;
    selectionBeforeDrag = new Set(selectedIds);

    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("mouseup", onDragEnd);
    window.addEventListener("blur", stopDragSelection);
    window.addEventListener("contextmenu", stopDragSelection);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    event.preventDefault();
  }

  function onDragMove(event: MouseEvent) {
    if (!dragActive) return;
    dragCurrentX = event.clientX;
    dragCurrentY = event.clientY;

    // Determine selection rect
    const left = Math.min(dragStartX, dragCurrentX);
    const right = Math.max(dragStartX, dragCurrentX);
    const top = Math.min(dragStartY, dragCurrentY);
    const bottom = Math.max(dragStartY, dragCurrentY);

    if (!profileListEl) return;

    const rows = profileListEl.querySelectorAll<HTMLElement>(".profile-row");
    const next = new Set(selectionBeforeDrag);

    rows.forEach((row) => {
      const rect = row.getBoundingClientRect();
      const assetId = row.dataset.assetId;
      if (!assetId) return;

      const intersects =
        rect.left < right && rect.right > left &&
        rect.top < bottom && rect.bottom > top;

      if (intersects) {
        if (selectionBeforeDrag.has(assetId)) {
          next.delete(assetId);
        } else {
          next.add(assetId);
        }
      } else {
        // Restore to pre-drag state
        if (selectionBeforeDrag.has(assetId)) {
          next.add(assetId);
        } else {
          next.delete(assetId);
        }
      }
    });

    syncSelectedIds(orderedSelection(next));
  }

  function onDragEnd() {
    stopDragSelection();
  }

  onDestroy(() => {
    stopDragSelection();
  });

  $: dragRectStyle = dragActive
    ? `left:${Math.min(dragStartX, dragCurrentX)}px;top:${Math.min(dragStartY, dragCurrentY)}px;width:${Math.abs(dragCurrentX - dragStartX)}px;height:${Math.abs(dragCurrentY - dragStartY)}px`
    : "";

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
      tRaw("batch.awaitingSelection")
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
  $: downloadedIdSet = new Set(downloadedAssetIds);
  $: analysisCounterLabel = analysisProgress
    ? analysisProgress.total > 0
      ? `已读取 ${analysisProgress.current} / ${analysisProgress.total} 个${itemLabel}`
      : `已读取 ${analysisProgress.current} 个${itemLabel}，正在统计总数…`
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
          <h3>{$t("batch.downloadContent")}</h3>
        </div>
        <span class="chip subtle">
          {hasSelectedDownloadOptions(downloadOptions)
            ? summarizeDownloadOptions(downloadOptions)
            : $t("common.notSelected")}
        </span>
      </div>

      <div class="option-grid">
        <label class="option-chip">
          <input bind:checked={downloadOptions.downloadVideo} type="checkbox" onchange={() => { if (!downloadOptions.downloadVideo) downloadOptions.downloadAudio = false; }} />
          <span>{$t("content.video")}</span>
        </label>
        <label class="option-chip" class:subtle-option={!downloadOptions.downloadVideo}>
          <input bind:checked={downloadOptions.downloadAudio} type="checkbox" disabled={!downloadOptions.downloadVideo} />
          <span>{$t("content.audio")}</span>
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
        onclick={() => dispatch("enqueue")}
        disabled={!preview || enqueuing || !selectedIds.length}
      >
        {enqueuing ? enqueuingLabel : enqueueLabel}
      </button>
    </div>

  </article>

  {#if preview}
    <article class="panel profile-panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">{resultEyebrow}</p>
          <h3>{preview.profileTitle}</h3>
        </div>

        <div class="section-actions">
          <span class="chip subtle">{$t("batch.selected")} {selectedIds.length} / {preview.items.length} {$t("common.unit")}{itemLabel}</span>
          <button class="text-button" onclick={selectAllItems} type="button">
            {$t("common.selectAll")}
          </button>
          <button class="text-button" onclick={invertSelection} type="button">
            {$t("batch.invertSelection")}
          </button>
          <button class="text-button" onclick={clearSelectedItems} type="button">
            {$t("batch.clearSelection")}
          </button>
          <button class="text-button" onclick={() => dispatch("close")} type="button">
            {$t("common.close")}
          </button>
        </div>
      </div>

      <div class="profile-toolbar">
        <div class="meta-row">
          <span class="meta-item">{$t("batch.fetched")} {preview.fetchedCount} {$t("common.unit")}{itemLabel}</span>
          <span class="meta-item">{$t("batch.selected")} {selectedIds.length} {$t("common.unit")}</span>
          {#each categoryStats as stat}
            <span class="meta-item subtle">{stat}</span>
          {/each}
        </div>

        <label class="search-field">
          <input
            bind:value={filterText}
            class="settings-input"
            placeholder={`${$t("batch.filterPlaceholder")} ${itemLabel}`}
            type="text"
          />
        </label>
      </div>

      <div class="profile-list" bind:this={profileListEl} use:attachDragSelection>
        {#if dragActive}
          <div class="drag-select-rect" style={dragRectStyle}></div>
        {/if}
        {#if filteredItems.length === 0}
          <p class="empty-state">{$t("batch.noMatch")}{itemLabel}</p>
        {:else}
          {#each filteredItems as item, index (item.assetId)}
            {@const selected = selectedIdSet.has(item.assetId)}
            <div class:selected-row={selected} class="profile-row" data-asset-id={item.assetId}>
              <div class="profile-check">
                {#key `${item.assetId}:${selected ? "1" : "0"}`}
                  <input
                    checked={selected}
                    onclick={(e) => trackShiftKey(e)}
                    onchange={(event) => toggleSelection(event, item.assetId, index)}
                    type="checkbox"
                  />
                {/key}
              </div>

              <div class="profile-copy">
                {#if thumbnailCache[item.assetId]}
                  <img src={thumbnailCache[item.assetId]} alt={item.title} class="profile-thumb" />
                {/if}
                <div class="profile-text">
                  <strong>{item.title}</strong>
                  <span>
                    {itemMetaLine(item)}
                    {#if downloadedIdSet.has(item.assetId)}
                      <span class="mini-tag accent" style="margin-left: 6px; display: inline-flex; font-size: 0.72rem;">{$t("common.alreadyDownloaded")}</span>
                    {/if}
                  </span>
                </div>
              </div>

              {#if downloadOptions.downloadVideo && hasFormats(item)}
                <div class="profile-format-slot">
                  <small>{$t("batch.formatLabel")}：{selectedFormat(item, selectedFormatIdsByAssetId[item.assetId] ?? currentFormatId(item), authState)?.label ?? pickPreferredFormat(item, "recommended", authState)?.label ?? tRaw("batch.awaitingSelection")}</small>
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
