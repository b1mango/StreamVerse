<script lang="ts">
  import { Check } from "@lucide/svelte";
  import type { DownloadContentSelection } from "../types";
  import { t } from "../i18n";

  let {
    options,
    onChange,
    label = "下载内容"
  }: {
    options: DownloadContentSelection;
    onChange: (options: DownloadContentSelection) => void;
    label?: string;
  } = $props();

  const keys: (keyof DownloadContentSelection)[] = ["downloadVideo", "downloadAudio", "downloadCover", "downloadCaption"];
  const labels: Record<keyof DownloadContentSelection, string> = {
    downloadVideo: "content.video",
    downloadAudio: "content.audio",
    downloadCover: "content.cover",
    downloadCaption: "content.caption"
  };
</script>

<fieldset class="content-options" aria-label={label}>
  <legend>{label}</legend>
  <div class="option-chips">
    {#each keys as key}
      <button
        class="option-chip"
        class:on={options[key]}
        type="button"
        role="checkbox"
        aria-checked={options[key]}
        onclick={() => onChange({ ...options, [key]: !options[key] })}
      >
        <span class="chip-check">{#if options[key]}<Check size={12} />{/if}</span>
        {$t(labels[key])}
      </button>
    {/each}
  </div>
</fieldset>
