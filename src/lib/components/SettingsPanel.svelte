<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { platformMeta } from "../options";
  import type {
    LanguageCode,
    PlatformAuthDraft,
    PlatformAuthProfile,
    PlatformId,
    QualityPreference,
    ThemeMode
  } from "../types";
  import { t, setLanguage } from "../i18n";

  export let open = false;
  export let saveDirectoryDraft = "";
  export let platformAuthDrafts: Record<PlatformId, PlatformAuthDraft> = {
    douyin: { cookieBrowser: null, cookieFile: null, cookieText: null },
    bilibili: { cookieBrowser: null, cookieFile: null, cookieText: null },
    youtube: { cookieBrowser: null, cookieFile: null, cookieText: null }
  };
  export let platformAuthProfiles: Record<PlatformId, PlatformAuthProfile> = {
    douyin: { authState: "guest", accountLabel: "未登录", cookieBrowser: null, cookieFile: null },
    bilibili: { authState: "guest", accountLabel: "未登录", cookieBrowser: null, cookieFile: null },
    youtube: { authState: "guest", accountLabel: "未登录", cookieBrowser: null, cookieFile: null }
  };
  export let qualityPreference: QualityPreference = "recommended";
  export let autoRevealInFinder = false;
  export let accountLabel = "未登录";
  export let ffmpegAvailable = false;
  export let isWindows = false;
  export let settingsSaving = false;
  export let pickingDirectory = false;
  export let pickingCookieFilePlatform: PlatformId | null = null;
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

  const dispatch = createEventDispatcher<{
    close: void;
    save: void;
    pickDirectory: void;
    pickCookieFile: { platform: PlatformId };
  }>();

  const authPlatforms: PlatformId[] = ["douyin", "bilibili", "youtube"];

  function authDraft(platform: PlatformId) {
    return platformAuthDrafts[platform];
  }

  function authProfile(platform: PlatformId) {
    return platformAuthProfiles[platform];
  }

  function updateAuthDraft(
    platform: PlatformId,
    field: keyof PlatformAuthDraft,
    value: string | null
  ) {
    platformAuthDrafts = {
      ...platformAuthDrafts,
      [platform]: {
        ...platformAuthDrafts[platform],
        [field]: value
      }
    };
  }

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

      <div class="settings-auth-block">
        <div>
          <span class="settings-label">{$t('settings.platformAuth')}</span>
          <p class="settings-hint">{$t('settings.platformAuthHint')}</p>
        </div>

        {#each authPlatforms as platform}
          <section class="settings-auth-card">
            <div class="settings-auth-head">
              <div>
                <strong>{platformMeta[platform].label}</strong>
                <p class="settings-hint">
                  {$t('settings.currentStatus')}：{authProfile(platform).accountLabel}
                </p>
              </div>
              <span class="chip subtle">{$t(`auth.${authProfile(platform).authState}`)}</span>
            </div>

            <label class="settings-field">
              <span class="settings-label">{$t('settings.browserSource')}</span>
              <select
                class="settings-input"
                value={authDraft(platform).cookieBrowser ?? ""}
                onchange={(event) =>
                  updateAuthDraft(
                    platform,
                    'cookieBrowser',
                    (event.currentTarget as HTMLSelectElement).value || null
                  )}
              >
                {#each browserOptions as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>

            <label class="settings-field">
              <span class="settings-label">{$t('settings.cookieFile')}</span>
              <input
                class="settings-input"
                placeholder={$t('settings.cookieFilePlaceholder')}
                type="text"
                value={authDraft(platform).cookieFile ?? ""}
                oninput={(event) =>
                  updateAuthDraft(
                    platform,
                    'cookieFile',
                    (event.currentTarget as HTMLInputElement).value || null
                  )}
              />
            </label>

            <div class="settings-actions">
              <button
                class="secondary-button"
                onclick={() => dispatch('pickCookieFile', { platform })}
                disabled={Boolean(pickingCookieFilePlatform) || settingsSaving}
              >
                {pickingCookieFilePlatform === platform
                  ? $t('settings.pickingCookieFile')
                  : $t('settings.pickCookieFile')}
              </button>
            </div>

            {#if isWindows && platform !== 'youtube'}
              <div class="settings-guide-card compact-guide">
                <strong>{$t('settings.cookieGuideTitle')}</strong>
                <p>{$t('settings.cookieGuideBody')}</p>
              </div>

              <label class="settings-field">
                <span class="settings-label">{$t('settings.cookieText')}</span>
                <textarea
                  class="settings-input settings-textarea"
                  placeholder={$t('settings.cookieTextPlaceholder')}
                  rows="5"
                  value={authDraft(platform).cookieText ?? ""}
                  oninput={(event) =>
                    updateAuthDraft(
                      platform,
                      'cookieText',
                      (event.currentTarget as HTMLTextAreaElement).value || null
                    )}
                ></textarea>
              </label>
            {/if}
          </section>
        {/each}
      </div>

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
        <small class="settings-hint">{$t('settings.proxyHintYoutube')}</small>
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
