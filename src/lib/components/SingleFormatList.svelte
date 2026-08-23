<script lang="ts">
  import { ChevronDown, ChevronUp } from "@lucide/svelte";
  import { estimateFileSize, formatFileSize } from "../media";
  import { t } from "../i18n";
  import type { VideoFormat } from "../types";

  let {
    formats,
    selectedId,
    expanded,
    durationSeconds,
    onSelect,
    onExpandedChange
  }: {
    formats: VideoFormat[];
    selectedId: string;
    expanded: boolean;
    durationSeconds: number;
    onSelect: (formatId: string) => void;
    onExpandedChange: (expanded: boolean) => void;
  } = $props();

  let visibleFormats = $derived(expanded ? formats : formats.slice(0, 10));
  let hiddenCount = $derived(Math.max(0, formats.length - 10));

  function sizeLabel(format: VideoFormat) {
    if (format.fileSizeBytes && format.fileSizeBytes > 0) {
      return formatFileSize(format.fileSizeBytes);
    }
    const estimate = estimateFileSize(format.bitrateKbps, durationSeconds);
    return estimate ? `≈ ${formatFileSize(estimate)}` : "--";
  }
</script>

<div class="format-list" role="radiogroup" aria-label={$t("batch.formatLabel")}>
  <div class="format-list-header" aria-hidden="true">
    <span></span>
    <span>{$t("format.quality")}</span>
    <span>{$t("format.codec")}</span>
    <span>{$t("format.container")}</span>
    <span>{$t("format.fileSize")}</span>
  </div>
  {#each visibleFormats as format (format.id)}
    <button
      class:active={selectedId === format.id}
      type="button"
      role="radio"
      aria-checked={selectedId === format.id}
      onclick={() => onSelect(format.id)}
    >
      <span class="format-marker" aria-hidden="true"></span>
      <strong>{format.label}</strong>
      <span>{format.codec}</span>
      <span>{format.container}</span>
      <small>{sizeLabel(format)}</small>
    </button>
  {/each}
</div>

{#if hiddenCount > 0}
  <button
    class="format-expand-button"
    type="button"
    aria-expanded={expanded}
    onclick={() => onExpandedChange(!expanded)}
  >
    {#if expanded}<ChevronUp size={15} />{$t("format.collapse")}{:else}<ChevronDown size={15} />{$t("format.expand")} {hiddenCount}{/if}
  </button>
{/if}
