<script lang="ts">
  import { Check, ChevronRight, Download, FileText, FolderOpen, Globe, Info, KeyRound, LoaderCircle, Palette, RefreshCw, ShieldCheck, Sparkles, Trash2, X } from "@lucide/svelte";
  import { platformMeta, qualityOptions } from "../options";
  import { t } from "../i18n";
  import { resolveErrorMessage } from "../media";
  import { checkForUpdate, openExternalUrl } from "../backend";
  import Select from "./Select.svelte";
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
  let section = $state<"auth" | "download" | "appearance" | "about">("auth");
  let initializedOpen = false;

  const GITHUB_REPO = "https://github.com/b1mango/StreamVerse";
  let updateStatus = $state<"idle" | "checking" | "latest" | "available" | "failed">("idle");
  let updateLatest = $state("");
  let updateUrl = $state(`${GITHUB_REPO}/releases`);

  // 后端会 clamp 到 1–8，输入超界时提前给出提示
  const concurrentOutOfRange = $derived(maxConcurrentDownloads > 8 || maxConcurrentDownloads < 1);

  async function checkUpdate() {
    if (updateStatus === "checking") return;
    updateStatus = "checking";
    try {
      const result = await checkForUpdate();
      updateLatest = result.latestVersion;
      updateUrl = result.releaseUrl;
      updateStatus = result.hasUpdate ? "available" : "latest";
    } catch {
      updateStatus = "failed";
    }
  }

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
      maxConcurrentDownloads: Math.min(8, Math.max(1, Math.round(maxConcurrentDownloads) || 1)),
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
  <div class="settings-dialog" role="dialog" aria-label={$t("app.settings")} aria-modal="true">
    <aside class="settings-nav">
      <div class="settings-nav-head"><span class="eyebrow">{$t("settings.title")}</span></div>
      <button class:active={section === "auth"} type="button" onclick={() => (section = "auth")}><KeyRound size={16} />{$t("settings.platformAuth")}</button>
      <button class:active={section === "download"} type="button" onclick={() => (section = "download")}><Download size={16} />{$t("common.download")}</button>
      <button class:active={section === "appearance"} type="button" onclick={() => (section = "appearance")}><Palette size={16} />{$t("settings.appearance")}</button>
      <button class:active={section === "about"} type="button" onclick={() => (section = "about")}><Info size={16} />{$t("settings.about")}</button>
    </aside>

    <div class="settings-body">
      <header class="settings-body-head">
        <h2>{{ auth: $t("settings.platformAuth"), download: $t("common.download"), appearance: $t("settings.appearance"), about: $t("settings.about") }[section]}</h2>
        <button class="icon-button" type="button" title={$t("settings.close")} aria-label={$t("settings.close")} onclick={onClose}><X size={18} /></button>
      </header>

      <div class="settings-scroll">
        {#if section === "auth"}
          <section class="settings-section">
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

            <h3 class="auth-group-title">{$t("settings.autoFetch")}</h3>
            <label>{$t("settings.browserSource")}
              <Select value={browserId} options={browserSources.map((source) => ({ value: source.id, label: `${source.label}${source.isDefault ? " · 默认" : ""}` }))} onChange={(next) => { browserId = next; profileId = selectedBrowser()?.profiles.find((profile) => profile.isDefault)?.id ?? selectedBrowser()?.profiles[0]?.id ?? ""; }} />
            </label>
            <label>Profile
              <Select value={profileId} options={(selectedBrowser()?.profiles ?? []).map((profile) => ({ value: profile.id, label: `${profile.label}${profile.isDefault ? " · 推荐" : ""}` }))} onChange={(next) => (profileId = next)} />
            </label>
            <button class="primary-button" type="button" disabled={!browserId || busy} onclick={() => (consentOpen = true)}><ShieldCheck size={17} />{$t("settings.detectCookie")}</button>

            <div class="manual-auth">
              <h3 class="auth-group-title">{$t("settings.manualFetch")}</h3>
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
        {:else if section === "download"}
          <section class="settings-section">
            <p class="section-desc">{$t("settings.downloadControls")}</p>
            <label>{$t("settings.downloadPath")}
              <div class="input-action"><input bind:value={saveDirectory} /><button class="icon-button" type="button" title={$t("settings.pickDirectory")} aria-label={$t("settings.pickDirectory")} onclick={async () => { const path = await onPickDirectory(); if (path) saveDirectory = path; }}><FolderOpen size={16} /></button></div>
            </label>
            <div class="two-columns">
              <label>{$t("settings.qualityStrategy")}<Select value={qualityPreference} options={qualityOptions} onChange={(next) => (qualityPreference = next as BootstrapState["qualityPreference"])} /></label>
              <label>{$t("settings.maxConcurrent")}<input type="number" min="1" max="8" bind:value={maxConcurrentDownloads} /></label>
            </div>
            {#if concurrentOutOfRange}<p class="inline-message error" role="alert">{$t("settings.maxConcurrentHint")}</p>{/if}
            <div class="two-columns">
              <label>{$t("settings.proxy")}<input bind:value={proxyUrl} placeholder="http://127.0.0.1:7890" /></label>
              <label>{$t("settings.speedLimit")}<input bind:value={speedLimit} placeholder="8M" /></label>
            </div>
            <label class="toggle-line"><input type="checkbox" bind:checked={autoRevealInFinder} /><span>{$t("settings.autoReveal")}</span></label>
            <label class="toggle-line"><input type="checkbox" bind:checked={notifyOnComplete} /><span>{$t("settings.notifyOnComplete")}</span></label>
          </section>
        {:else if section === "appearance"}
          <section class="settings-section compact-settings">
            <label>{$t("settings.theme")}<Select value={theme} options={[{ value: "dark", label: $t("theme.dark") }, { value: "light", label: $t("theme.light") }]} onChange={(next) => (theme = next as BootstrapState["theme"])} /></label>
            <label>{$t("settings.language")}<Select value={language} options={[{ value: "zh-CN", label: "简体中文" }, { value: "en", label: "English" }]} onChange={(next) => (language = next as BootstrapState["language"])} /></label>
          </section>
        {:else}
          <section class="settings-section">
            <div class="about-card">
              <span class="about-logo"><Sparkles size={19} /></span>
              <div class="about-name"><strong>StreamVerse</strong><span>v{bootstrap.version}</span></div>
            </div>
            <div class="about-row">
              <span>{$t("settings.repository")}</span>
              <button class="link-button" type="button" onclick={() => void openExternalUrl(GITHUB_REPO).catch(() => undefined)}><Globe size={13} style="vertical-align: -2px;" /> github.com/b1mango/StreamVerse</button>
            </div>
            <div class="about-row">
              <span>{$t("settings.version")} v{bootstrap.version}</span>
              <button class="quiet-button" type="button" disabled={updateStatus === "checking"} onclick={checkUpdate}>{#if updateStatus === "checking"}<LoaderCircle class="spin" size={15} />{$t("settings.checkingUpdate")}{:else}<RefreshCw size={15} />{$t("settings.checkUpdate")}{/if}</button>
            </div>
            {#if updateStatus !== "idle" && updateStatus !== "checking"}
              <p class="update-status" class:error={updateStatus === "failed"} role="status">
                {#if updateStatus === "latest"}{$t("settings.updateLatest")}（v{updateLatest}）
                {:else if updateStatus === "available"}{$t("settings.updateAvailable")}：v{updateLatest} <button class="link-button" type="button" onclick={() => void openExternalUrl(updateUrl).catch(() => undefined)}>{$t("settings.viewRelease")}</button>
                {:else}{$t("settings.updateFailed")}{/if}
              </p>
            {/if}
          </section>
        {/if}
      </div>

      {#if section !== "about"}
        <footer class="sheet-footer"><button class="primary-button" type="button" disabled={busy} onclick={submitSettings}>{#if busy}<LoaderCircle class="spin" size={17} />{:else}<Check size={17} />{/if}{$t("settings.save")}</button></footer>
      {/if}
    </div>
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
