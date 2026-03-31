<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { QualityPreference } from "../types";

  export let open = false;
  export let saveDirectoryDraft = "";
  export let cookieBrowser = "";
  export let qualityPreference: QualityPreference = "recommended";
  export let autoRevealInFinder = false;
  export let accountLabel = "未登录";
  export let ffmpegAvailable = false;
  export let settingsSaving = false;
  export let pickingDirectory = false;
  export let browserOptions: Array<{ value: string; label: string }> = [];
  export let qualityOptions: Array<{ value: QualityPreference; label: string }> = [];

  const dispatch = createEventDispatcher<{ close: void; save: void; pickDirectory: void }>();
</script>

{#if open}
  <button
    aria-label="关闭设置"
    class="settings-overlay"
    onclick={() => dispatch("close")}
    type="button"
  ></button>
  <aside class="panel settings-panel">
    <div class="section-head">
      <div>
        <p class="eyebrow">Settings</p>
        <h3>应用设置</h3>
      </div>
      <button class="ghost-button" onclick={() => dispatch("close")}>关闭</button>
    </div>

    <div class="settings-stack">
      <label class="settings-field">
        <span class="settings-label">默认下载路径</span>
        <input
          bind:value={saveDirectoryDraft}
          class="settings-input"
          placeholder="选择或输入默认下载目录"
          type="text"
        />
      </label>

      <div class="settings-actions">
        <button
          class="secondary-button"
          onclick={() => dispatch("pickDirectory")}
          disabled={pickingDirectory || settingsSaving}
        >
          {pickingDirectory ? "选择中…" : "选择目录"}
        </button>
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

      <div class="settings-status">
        <span class="meta-item">{accountLabel}</span>
        <span class="meta-item">FFmpeg：{ffmpegAvailable ? "已检测到" : "未检测到"}</span>
      </div>

      <div class="settings-submit">
        <button
          class="primary-button"
          onclick={() => dispatch("save")}
          disabled={settingsSaving || pickingDirectory}
        >
          {settingsSaving ? "保存中…" : "保存设置"}
        </button>
      </div>
    </div>
  </aside>
{/if}
