<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { QualityPreference, ThemeMode, LanguageCode } from "../types";
  import { t, setLanguage } from "../i18n";

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

  export let maxConcurrentDownloads = 3;
  export let proxyUrl = "";
  export let speedLimit = "";
  export let autoUpdate = false;
  export let theme: ThemeMode = "dark";
  export let notifyOnComplete = true;
  export let language: LanguageCode = "zh-CN";

  const themeOptions: Array<{ value: ThemeMode; label: string }> = [
    { value: "dark", label: "深色模式" },
    { value: "light", label: "浅色模式" }
  ];

  const languageOptions: Array<{ value: LanguageCode; label: string }> = [
    { value: "zh-CN", label: "简体中文" },
    { value: "en", label: "English" }
  ];

  const dispatch = createEventDispatcher<{ close: void; save: void; pickDirectory: void }>();

  function previewTheme(mode: ThemeMode) {
    const root = document.documentElement;
    root.classList.add("no-transition");
    root.setAttribute("data-theme", mode);
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        root.classList.remove("no-transition");
      });
    });
  }

  function previewLanguage(code: LanguageCode) {
    setLanguage(code);
  }
</script>

{#if open}
  <button
    aria-label={$t('settings.close')}
    class="settings-overlay"
    onclick={() => dispatch("close")}
    type="button"
  ></button>
  <aside class="panel settings-panel">
    <div class="section-head">
      <div>
        <p class="eyebrow">Settings</p>
        <h3>{$t('settings.title')}</h3>
      </div>
      <button class="ghost-button" onclick={() => dispatch("close")}>{$t('settings.close')}</button>
    </div>

    <div class="settings-stack">
      <label class="settings-field">
        <span class="settings-label">{$t('settings.downloadPath')}</span>
        <input
          bind:value={saveDirectoryDraft}
          class="settings-input"
          placeholder={$t('settings.downloadPathPlaceholder')}
          type="text"
        />
      </label>

      <div class="settings-actions">
        <button
          class="secondary-button"
          onclick={() => dispatch("pickDirectory")}
          disabled={pickingDirectory || settingsSaving}
        >
          {pickingDirectory ? $t('settings.pickingDirectory') : $t('settings.pickDirectory')}
        </button>
      </div>

      <label class="settings-field">
        <span class="settings-label">{$t('settings.loginSource')}</span>
        <select class="settings-input" bind:value={cookieBrowser}>
          {#each browserOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <label class="settings-field">
        <span class="settings-label">{$t('settings.qualityStrategy')}</span>
        <select class="settings-input" bind:value={qualityPreference}>
          {#each qualityOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <label class="checkbox-row">
        <input bind:checked={autoRevealInFinder} type="checkbox" />
        <span>{$t('settings.autoReveal')}</span>
      </label>

      <hr class="settings-divider" />

      <label class="settings-field">
        <span class="settings-label">{$t('settings.maxConcurrent')}</span>
        <input
          bind:value={maxConcurrentDownloads}
          class="settings-input"
          type="number"
          min="1"
          max="10"
        />
      </label>

      <label class="settings-field">
        <span class="settings-label">{$t('settings.proxy')}</span>
        <input
          bind:value={proxyUrl}
          class="settings-input"
          placeholder={$t('settings.proxyPlaceholder')}
          type="text"
        />
      </label>

      <label class="settings-field">
        <span class="settings-label">{$t('settings.speedLimit')}</span>
        <input
          bind:value={speedLimit}
          class="settings-input"
          placeholder={$t('settings.speedLimitPlaceholder')}
          type="text"
        />
      </label>

      <hr class="settings-divider" />

      <label class="settings-field">
        <span class="settings-label">{$t('settings.theme')}</span>
        <select
          class="settings-input"
          bind:value={theme}
          onchange={(e) => previewTheme((e.currentTarget as HTMLSelectElement).value as ThemeMode)}
        >
          {#each themeOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <label class="settings-field">
        <span class="settings-label">{$t('settings.language')}</span>
        <select
          class="settings-input"
          bind:value={language}
          onchange={(e) => previewLanguage((e.currentTarget as HTMLSelectElement).value as LanguageCode)}
        >
          {#each languageOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <label class="checkbox-row">
        <input bind:checked={notifyOnComplete} type="checkbox" />
        <span>{$t('settings.notifyOnComplete')}</span>
      </label>

      <label class="checkbox-row">
        <input bind:checked={autoUpdate} type="checkbox" />
        <span>{$t('settings.autoUpdate')}</span>
      </label>

      <div class="settings-status">
        <span class="meta-item">{accountLabel}</span>
        <span class="meta-item">FFmpeg: {ffmpegAvailable ? $t('settings.ffmpegDetected') : $t('settings.ffmpegMissing')}</span>
      </div>

      <div class="settings-submit">
        <button
          class="primary-button"
          onclick={() => dispatch("save")}
          disabled={settingsSaving || pickingDirectory}
        >
          {settingsSaving ? $t('settings.saving') : $t('settings.save')}
        </button>
      </div>
    </div>
  </aside>
{/if}
