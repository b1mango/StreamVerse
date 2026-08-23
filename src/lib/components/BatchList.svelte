<script lang="ts">
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import { Check, ListChecks } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { t } from "../i18n";
  import { visibleFormats } from "../media";
  import type { VideoAsset } from "../types";

  let {
    items,
    selectedIds,
    selectedFormats,
    onToggle,
    onFormat
  }: {
    items: VideoAsset[];
    selectedIds: Set<string>;
    selectedFormats: Record<string, string>;
    onToggle: (id: string, index: number, shift: boolean) => void;
    onFormat: (id: string, formatId: string) => void;
  } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => viewport,
    estimateSize: () => 72,
    overscan: 8
  });

  $effect(() => {
    const count = items.length;
    const scrollElement = viewport;
    // setOptions publishes back to the virtualizer store; do not subscribe this effect to that write.
    untrack(() => {
      $virtualizer.setOptions({ count, getScrollElement: () => scrollElement });
    });
  });
</script>

{#snippet row(item: VideoAsset, index: number)}
  {@const formats = visibleFormats(item, "active")}
  {@const imageCount = item.imageUrls?.length ?? 0}
  <div class:selected={selectedIds.has(item.assetId)} class="batch-row" data-index={index}>
    <button
      class="check-button"
      class:checked={selectedIds.has(item.assetId)}
      type="button"
      aria-label={`${selectedIds.has(item.assetId) ? $t("common.deselect") : $t("common.select")} ${item.title}`}
      title={selectedIds.has(item.assetId) ? $t("common.deselect") : $t("common.select")}
      onclick={(event) => onToggle(item.assetId, index, event.shiftKey)}
    >
      {#if selectedIds.has(item.assetId)}<Check size={15} />{/if}
    </button>
    <div class="batch-index">{String(index + 1).padStart(2, "0")}</div>
    <div class="batch-copy">
      <strong title={item.title}>{item.title}</strong>
      <span>{imageCount > 0 ? `${$t("content.album")} · ${imageCount} ${$t("content.images")}` : item.author || "Unknown"} · {item.publishDate || "--"}</span>
    </div>
    <select
      aria-label={`${item.title} ${$t("batch.formatLabel")}`}
      value={selectedFormats[item.assetId] ?? formats.find((format) => format.recommended)?.id ?? formats[0]?.id ?? ""}
      disabled={item.formatStatus === "pending" || item.formatStatus === "failed"}
      onchange={(event) => onFormat(item.assetId, event.currentTarget.value)}
    >
      {#if imageCount > 0}
        <option value="">{$t("content.album")} · {imageCount} {$t("content.images")}</option>
      {:else}
        {#each formats as format}
          <option value={format.id}>{format.label} · {format.resolution} · {format.codec}</option>
        {/each}
        {#if formats.length === 0}
          <option value="">{item.formatStatus === "pending" ? $t("batch.loadingFormats") : item.formatStatus === "failed" ? $t("batch.formatLoadFailed") : $t("common.auto")}</option>
        {/if}
      {/if}
    </select>
  </div>
{/snippet}

{#if items.length === 0}
  <div class="empty-list"><ListChecks size={22} /><span>{$t("batch.empty")}</span></div>
{:else if items.length > 50}
  <div class="batch-viewport" bind:this={viewport}>
    <div class="virtual-space" style:height={`${$virtualizer.getTotalSize()}px`}>
      {#each $virtualizer.getVirtualItems() as virtualRow (virtualRow.key)}
        <div
          class="virtual-row"
          style:transform={`translateY(${virtualRow.start}px)`}
          data-index={virtualRow.index}
        >
          {@render row(items[virtualRow.index], virtualRow.index)}
        </div>
      {/each}
    </div>
  </div>
{:else}
  <div class="batch-static">
    {#each items as item, index (item.assetId)}{@render row(item, index)}{/each}
  </div>
{/if}
