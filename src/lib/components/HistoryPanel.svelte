<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { t } from "../i18n";
  import { listDownloadHistory, searchDownloadHistory } from "../backend";
  import { platformMeta } from "../options";
  import type { DownloadHistoryEntry, PlatformId } from "../types";

  export let open = false;

  const dispatch = createEventDispatcher<{
    close: void;
    openPlatform: { platform: PlatformId; url: string };
  }>();

  let entries: DownloadHistoryEntry[] = [];
  let loading = true;
  let searchQuery = "";
  let filterPlatform: string = "";
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => loadHistory());

  async function loadHistory(platform?: string) {
    loading = true;
    try {
      entries = await listDownloadHistory(200, platform || undefined);
    } catch {
      entries = [];
    } finally {
      loading = false;
    }
  }

  function onSearchInput() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(async () => {
      const q = searchQuery.trim();
      if (q) {
        try {
          entries = await searchDownloadHistory(q, 100);
        } catch {
          entries = [];
        }
      } else {
        loadHistory(filterPlatform || undefined);
      }
    }, 300);
  }

  function onFilterChange() {
    searchQuery = "";
    loadHistory(filterPlatform || undefined);
  }

  function formatDate(ts: string): string {
    const secs = parseInt(ts, 10);
    if (!secs) return "";
    const d = new Date(secs * 1000);
    return d.toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit"
    });
  }

  function platformLabel(p: string): string {
    return platformMeta[p as PlatformId]?.label ?? p;
  }
</script>

{#if open}
<article class="panel history-panel">
  <div class="section-head">
    <div>
      <h3>{$t("history.title")}</h3>
    </div>
    <div class="section-actions">
      <button class="ghost-button" onclick={() => dispatch("close")}>
        {$t("settings.close")}
      </button>
    </div>
  </div>

  <div class="history-toolbar">
    <input
      class="settings-input"
      type="text"
      placeholder={$t("history.searchPlaceholder")}
      bind:value={searchQuery}
      oninput={onSearchInput}
    />
    <select class="settings-input" style="width:auto" bind:value={filterPlatform} onchange={onFilterChange}>
      <option value="">{$t("history.allPlatforms")}</option>
      <option value="douyin">{platformMeta.douyin.label}</option>
      <option value="bilibili">{platformMeta.bilibili.label}</option>
      <option value="youtube">{platformMeta.youtube.label}</option>
    </select>
  </div>

  <div class="history-meta">
    <span class="chip subtle">{entries.length} {$t("history.entries")}</span>
  </div>

  {#if loading}
    <div style="text-align:center;padding:24px">
      <span class="spinner"></span>
    </div>
  {:else if entries.length === 0}
    <p class="empty-state">{$t("history.empty")}</p>
  {:else}
    <div class="history-list">
      {#each entries as entry}
        <div class="history-row">
          <div class="history-copy">
            <strong>{entry.title}</strong>
            <span>{platformLabel(entry.platform)} · {formatDate(entry.downloadedAt)}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</article>

<style>
  .history-panel {
    margin-bottom: 18px;
  }

  .history-toolbar {
    display: flex;
    gap: 10px;
    margin-bottom: 12px;
  }

  .history-toolbar input {
    flex: 1;
  }

  .history-meta {
    margin-bottom: 14px;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 60vh;
    overflow-y: auto;
  }

  .history-row {
    display: flex;
    align-items: center;
    padding: 12px 14px;
    border-radius: var(--radius-sm);
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid var(--stroke);
  }

  .history-copy strong,
  .history-copy span {
    display: block;
  }

  .history-copy span {
    margin-top: 4px;
    color: var(--muted);
    font-size: 0.82rem;
  }
</style>
{/if}
