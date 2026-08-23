<script lang="ts">
  import { Check, ChevronRight, FileText, FolderOpen, KeyRound, LoaderCircle, ShieldCheck, Trash2, X } from "@lucide/svelte";
  import { platformMeta, qualityOptions } from "../options";
  import { t } from "../i18n";
  import { resolveErrorMessage } from "../media";
  import type {
    BootstrapState,
    BrowserSource,
    CookieImportResult,
    PlatformId,
    SaveSettingsPayload
  } from "../types";

  let {
    open,
    bootstrap,
    browserSources,
    busy = false,
    onClose,
    onSave,
    onPickDirectory,
    onPickCookieFile,
    onImportBrowser,
    onSaveManual,
    onImportCookieFile,
    onClearAuth
  }: {
    open: boolean;
    bootstrap: BootstrapState;
    browserSources: BrowserSource[];
    busy?: boolean;
    onClose: () => void;
    onSave: (payload: SaveSettingsPayload) => Promise<void>;
    onPickDirectory: () => Promise<string | null>;
    onPickCookieFile: () => Promise<string | null>;
    onImportBrowser: (platform: PlatformId, browserId: string, profileId: string | null, consent: "once" | "always", allowElevation: boolean) => Promise<CookieImportResult>;
    onSaveManual: (platform: PlatformId, value: string) => Promise<CookieImportResult>;
    onImportCookieFile: (platform: PlatformId, path: string) => Promise<CookieImportResult>;
    onClearAuth: (platform: PlatformId) => Promise<void>;
  } = $props();

  let platform = $state<PlatformId>("douyin");
  let browserId = $state("");
  let profileId = $state("");
  let manualCookie = $state("");
  let consentOpen = $state(false);
  let elevationOpen = $state(false);
  let authMessage = $state("");
  let authError = $state(false);
  let pendingConsent = $state<"once" | "always" | null>(null);
  let saveDirectory = $state("");
  let qualityPreference = $state<BootstrapState["qualityPreference"]>("recommended");
  let maxConcurrentDownloads = $state(3);
  let proxyUrl = $state("");
  let speedLimit = $state("");
  let autoRevealInFinder = $state(false);
  let notifyOnComplete = $state(true);
  let theme = $state<BootstrapState["theme"]>("dark");
  let language = $state<BootstrapState["language"]>("zh-CN");
  let initializedOpen = false;

  $effect(() => {
    if (open && !initializedOpen) {
      saveDirectory = bootstrap.saveDirectory;
      qualityPreference = bootstrap.qualityPreference;
      maxConcurrentDownloads = bootstrap.maxConcurrentDownloads;
      proxyUrl = bootstrap.proxyUrl ?? "";
      speedLimit = bootstrap.speedLimit ?? "";
      autoRevealInFinder = bootstrap.autoRevealInFinder;
      notifyOnComplete = bootstrap.notifyOnComplete;
      theme = bootstrap.theme;
      language = bootstrap.language;
      browserId = browserSources.find((source) => source.isDefault)?.id ?? browserSources[0]?.id ?? "";
      profileId = browserSources.find((source) => source.id === browserId)?.profiles.find((profile) => profile.isDefault)?.id ?? "";
      initializedOpen = true;
    } else if (!open) {
      initializedOpen = false;
      consentOpen = false;
      elevationOpen = false;
    }
  });

  const domains: Record<PlatformId, string> = {
    douyin: "douyin.com / iesdouyin.com",
    bilibili: "bilibili.com / b23.tv",
    youtube: "youtube.com / google.com"
  };

  function selectedBrowser() {
    return browserSources.find((source) => source.id === browserId);
  }

  async function importCookies(consent: "once" | "always", allowElevation = false) {
    if (pendingConsent) return;
    authMessage = "";
    authError = false;
    pendingConsent = consent;
    try {
      const result = await onImportBrowser(platform, browserId, profileId || null, consent, allowElevation);
      if (result.requiresElevation) {
        consentOpen = false;
        elevationOpen = true;
        authMessage = result.message;
        return;
      }
      consentOpen = false;
      elevationOpen = false;
      authMessage = result.message;
    } catch (error) {
      authError = true;
      authMessage = resolveErrorMessage(error);
    } finally {
      pendingConsent = null;
    }
  }

  async function submitSettings() {
    await onSave({
      saveDirectory,
      downloadMode: "manual",
      qualityPreference,
      autoRevealInFinder,
      maxConcurrentDownloads,
      proxyUrl: proxyUrl.trim() || null,
      speedLimit: speedLimit.trim() || null,
      autoUpdate: bootstrap.autoUpdate,
      theme,
      notifyOnComplete,
      language
    });
  }

  async function saveManual() {
    authMessage = "";
    authError = false;
    try {
      const result = await onSaveManual(platform, manualCookie);
      manualCookie = "";
      authMessage = result.message;
    } catch (error) {
      authError = true;
      authMessage = resolveErrorMessage(error);
    }
  }

  async function importCookieFile() {
    authMessage = "";
    authError = false;
    try {
      const path = await onPickCookieFile();
      if (!path) return;
      const result = await onImportCookieFile(platform, path);
      authMessage = result.message;
    } catch (error) {
      authError = true;
      authMessage = resolveErrorMessage(error);
    }
  }
</script>

{#if open}
  <button class="sheet-backdrop" type="button" aria-label={$t("settings.close")} onclick={onClose}></button>
  <div class="settings-sheet" role="dialog" aria-label={$t("app.settings")} aria-modal="true">
    <header class="sheet-header">
      <div><span class="eyebrow">CONTROL SURFACE</span><h2>{$t("app.settings")}</h2></div>
      <button class="icon-button" type="button" title={$t("settings.close")} aria-label={$t("settings.close")} onclick={onClose}><X size={19} /></button>
    </header>

    <div class="settings-scroll">
      <section class="settings-section">
        <div class="section-title"><KeyRound size={17} /><div><h3>{$t("settings.platformAuth")}</h3><p>{$t("settings.authStorage")}</p></div></div>
        <div class="platform-tabs" role="tablist" aria-label="认证平台">
          {#each Object.keys(platformMeta) as id}
            <button class:active={platform === id} type="button" role="tab" aria-selected={platform === id} onclick={() => { platform = id as PlatformId; authMessage = ""; authError = false; }}>{platformMeta[id as PlatformId].label}</button>
          {/each}
        </div>

        <div class="auth-status-line">
          <span class:active={bootstrap.platformAuth[platform].status === "active"}></span>
          <strong>{bootstrap.platformAuth[platform].status === "active" ? $t("auth.active") : $t("auth.guest")}</strong>
          {#if bootstrap.platformAuth[platform].browserId}<small>{bootstrap.platformAuth[platform].browserId}</small>{/if}
          {#if bootstrap.platformAuth[platform].status === "active"}
            <button class="icon-button" type="button" title={$t("settings.clearAuth")} aria-label={$t("settings.clearAuth")} onclick={() => onClearAuth(platform)}><Trash2 size={15} /></button>
          {/if}
        </div>

        <label>{$t("settings.browserSource")}
          <select bind:value={browserId} onchange={() => { profileId = selectedBrowser()?.profiles.find((profile) => profile.isDefault)?.id ?? selectedBrowser()?.profiles[0]?.id ?? ""; }}>
            {#each browserSources as source}<option value={source.id}>{source.label}{source.isDefault ? " · 默认" : ""}</option>{/each}
          </select>
        </label>
        <label>Profile
          <select bind:value={profileId}>
            {#each selectedBrowser()?.profiles ?? [] as profile}<option value={profile.id}>{profile.label}{profile.isDefault ? " · 推荐" : ""}</option>{/each}
          </select>
        </label>
        <button class="primary-button" type="button" disabled={!browserId || busy} onclick={() => (consentOpen = true)}><ShieldCheck size={17} />{$t("settings.detectCookie")}</button>

        <div class="manual-auth">
          <label>{$t("settings.cookieTextLabel")} / cookies.txt
            <textarea bind:value={manualCookie} rows="4" spellcheck="false" placeholder={$t("settings.cookieTextPlaceholder")}></textarea>
          </label>
          <div class="manual-auth-actions">
            <button class="quiet-button" type="button" disabled={busy} onclick={importCookieFile}><FileText size={16} />{$t("settings.importCookieFile")}</button>
            <button class="quiet-button" type="button" disabled={!manualCookie.trim() || busy} onclick={saveManual}><Check size={16} />{$t("settings.saveCookieText")}</button>
          </div>
        </div>
        {#if authMessage}<p class:error={authError} class="inline-message" aria-live="polite">{authMessage}</p>{/if}
      </section>

      <section class="settings-section">
        <div class="section-title"><FolderOpen size={17} /><div><h3>{$t("common.download")}</h3><p>{$t("settings.downloadControls")}</p></div></div>
        <label>{$t("settings.downloadPath")}
          <div class="input-action"><input bind:value={saveDirectory} /><button class="icon-button" type="button" title={$t("settings.pickDirectory")} aria-label={$t("settings.pickDirectory")} onclick={async () => { const path = await onPickDirectory(); if (path) saveDirectory = path; }}><FolderOpen size={16} /></button></div>
        </label>
        <div class="two-columns">
          <label>{$t("settings.qualityStrategy")}<select bind:value={qualityPreference}>{#each qualityOptions as option}<option value={option.value}>{option.label}</option>{/each}</select></label>
          <label>{$t("settings.maxConcurrent")}<input type="number" min="1" max="8" bind:value={maxConcurrentDownloads} /></label>
        </div>
        <div class="two-columns">
          <label>{$t("settings.proxy")}<input bind:value={proxyUrl} placeholder="http://127.0.0.1:7890" /></label>
          <label>{$t("settings.speedLimit")}<input bind:value={speedLimit} placeholder="8M" /></label>
        </div>
        <label class="toggle-line"><input type="checkbox" bind:checked={autoRevealInFinder} /><span>{$t("settings.autoReveal")}</span></label>
        <label class="toggle-line"><input type="checkbox" bind:checked={notifyOnComplete} /><span>{$t("settings.notifyOnComplete")}</span></label>
      </section>

      <section class="settings-section compact-settings">
        <label>{$t("settings.theme")}<select bind:value={theme}><option value="dark">Dark</option><option value="light">Light</option></select></label>
        <label>{$t("settings.language")}<select bind:value={language}><option value="zh-CN">简体中文</option><option value="en">English</option></select></label>
      </section>
    </div>
    <footer class="sheet-footer"><button class="primary-button" type="button" disabled={busy} onclick={submitSettings}>{#if busy}<LoaderCircle class="spin" size={17} />{:else}<Check size={17} />{/if}{$t("settings.save")}</button></footer>
  </div>

  {#if consentOpen}
    <div class="consent-dialog" role="dialog" aria-modal="true" aria-label={$t("settings.cookieConsent")}>
      <ShieldCheck size={24} />
      <h3>{$t("settings.allowRead")} {platformMeta[platform].label}</h3>
      <dl><dt>{$t("settings.browserSource")}</dt><dd>{selectedBrowser()?.label} / {selectedBrowser()?.profiles.find((item) => item.id === profileId)?.label}</dd><dt>{$t("settings.domainScope")}</dt><dd>{domains[platform]}</dd><dt>{$t("settings.storageLocation")}</dt><dd>{$t("settings.localOnly")}</dd></dl>
      <p>{$t("settings.cookieBoundary")}</p>
      {#if authError && authMessage}<p class="inline-message error" role="alert">{authMessage}</p>{/if}
      <div class="dialog-actions"><button class="quiet-button" type="button" disabled={busy || pendingConsent !== null} onclick={() => (consentOpen = false)}>{$t("common.cancel")}</button><button class="quiet-button" type="button" disabled={busy || pendingConsent !== null} onclick={() => importCookies("once")}>{#if pendingConsent === "once"}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t("settings.once")}{/if}</button><button class="primary-button" type="button" disabled={busy || pendingConsent !== null} onclick={() => importCookies("always")}>{#if pendingConsent === "always"}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t("settings.always")}<ChevronRight size={16} />{/if}</button></div>
    </div>
  {/if}

  {#if elevationOpen}
    <div class="consent-dialog" role="dialog" aria-modal="true" aria-label="提权 Cookie helper 确认">
      <ShieldCheck size={24} />
      <h3>{$t("settings.elevationTitle")}</h3>
      <p class="inline-message" class:error={authError} aria-live="assertive">{authMessage}</p><p>{$t("settings.elevationBoundary")}</p>
      <div class="dialog-actions"><button class="quiet-button" type="button" disabled={busy || pendingConsent !== null} onclick={() => (elevationOpen = false)}>{$t("common.cancel")}</button><button class="primary-button" type="button" disabled={busy || pendingConsent !== null} onclick={() => importCookies("always", true)}>{#if pendingConsent}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t("settings.continueUac")}<ChevronRight size={16} />{/if}</button></div>
    </div>
  {/if}
{/if}
