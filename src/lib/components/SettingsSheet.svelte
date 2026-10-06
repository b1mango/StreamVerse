<script lang="ts">
  import { Check, ChevronRight, Download, FileText, FolderOpen, Globe, Info, KeyRound, LoaderCircle, Palette, RefreshCw, ShieldCheck, Trash2, X } from "@lucide/svelte";
  import appIconUrl from "../../../src-tauri/icons/icon.png";
  import { platformMeta, qualityOptions } from "../options";
  import { t } from "../i18n";
  import { resolveErrorMessage } from "../media";
  import { checkForUpdate, isMacOS, openExternalUrl } from "../backend";
  import Select from "./Select.svelte";
  import type {
    BootstrapState,
    BrowserSource,
    BrowserCookieSyncRequest,
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
    onAuthorizeBrowser,
    onSyncBrowser,
    onSaveManual,
    onImportCookieFile,
    onClearAuth,
    onOpenLogin
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
    onAuthorizeBrowser: (browserId: string) => Promise<BrowserSource[]>;
    onSyncBrowser: (request: BrowserCookieSyncRequest) => Promise<CookieImportResult[]>;
    onSaveManual: (platform: PlatformId, value: string) => Promise<CookieImportResult>;
    onImportCookieFile: (platform: PlatformId, path: string) => Promise<CookieImportResult>;
    onClearAuth: (platform: PlatformId) => Promise<void>;
    onOpenLogin: (platform: PlatformId) => Promise<void>;
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
  let saveError = $state("");
  let speedLimit = $state("");
  let autoRevealInFinder = $state(false);
  let notifyOnComplete = $state(true);
  let theme = $state<BootstrapState["theme"]>("dark");
  let language = $state<BootstrapState["language"]>("zh-CN");
  let section = $state<"auth" | "download" | "appearance" | "about">("auth");
  let initializedOpen = false;
  let authBusy = $state(false);
  let consentTarget = $state<{ browserId: string; profileId: string; platforms: PlatformId[] } | null>(null);
  let elevationConsent = $state<"once" | "always">("once");
  const allPlatforms: PlatformId[] = ["douyin", "bilibili", "youtube"];
  const working = $derived(busy || authBusy || pendingConsent !== null);
  const authLocked = $derived(working || consentOpen || elevationOpen);
  const directoryAuthorization = $derived(isMacOS() && (browserId === "chrome" || browserId === "edge"));
  const profiles = $derived(selectedBrowser()?.degraded ? [] : selectedBrowser()?.profiles ?? []);
  const validProfile = $derived(profiles.some((profile) => profile.id === profileId && profile.id.trim() !== ""));

  const GITHUB_REPO = "https://github.com/b1mango/StreamVerse";
  let updateStatus = $state<"idle" | "checking" | "latest" | "available" | "failed">("idle");
  let updateLatest = $state("");
  let updateUrl = $state(`${GITHUB_REPO}/releases`);

  // 后端会 clamp 到 1–8，输入超界时提前给出提示
  const concurrentOutOfRange = $derived(maxConcurrentDownloads > 8 || maxConcurrentDownloads < 1);

  // 代理留空 = 自动识别：跟随 bootstrap 中后端探测到的生效代理展示状态
  const proxyAutoMode = $derived(!proxyUrl.trim());

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
      restorePlatformBrowser();
      initializedOpen = true;
    } else if (!open) {
      initializedOpen = false;
      consentOpen = false;
      elevationOpen = false;
    }
  });

  $effect(() => {
    if (open && !authLocked && !browserId && browserSources.length) {
      restorePlatformBrowser();
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

  function preferredProfile(source: BrowserSource | undefined, targetPlatform: PlatformId, allowDefault = true) {
    const available = source?.degraded ? [] : source?.profiles.filter((item) => item.id.trim()) ?? [];
    const saved = bootstrap.platformAuth[targetPlatform];
    const savedId = saved.browserId === source?.id ? available.find((item) => item.id === saved.profileId)?.id : undefined;
    return savedId ?? (available.length === 1 ? available[0].id : allowDefault ? available.find((item) => item.isDefault)?.id ?? available[0]?.id ?? "" : "");
  }

  function selectBrowser(next: string) {
    browserId = next;
    profileId = preferredProfile(browserSources.find((item) => item.id === next), platform);
  }

  function restorePlatformBrowser() {
    const savedBrowserId = bootstrap.platformAuth[platform].browserId;
    selectBrowser(browserSources.find((source) => source.id === savedBrowserId)?.id
      ?? browserSources.find((source) => source.isDefault)?.id ?? browserSources[0]?.id ?? "");
  }

  async function runAuth(action: () => Promise<void>) {
    if (authLocked) return;
    authBusy = true;
    authMessage = "";
    authError = false;
    try {
      await action();
    } catch (error) {
      authMessage = resolveErrorMessage(error);
      authError = true;
    } finally {
      authBusy = false;
    }
  }

  function showResult(result: CookieImportResult) {
    authMessage = result.message;
    authError = result.status !== "active";
  }

  async function authorizeDirectory() {
    if (!directoryAuthorization) return;
    const targetBrowser = browserId;
    const targetPlatform = platform;
    await runAuth(async () => {
      const sources = await onAuthorizeBrowser(targetBrowser);
      const source = sources.find((item) => item.id === targetBrowser);
      // Restore a saved real profile; otherwise require a choice when multiple profiles exist.
      profileId = preferredProfile(source, targetPlatform, false);
      if (!source?.directoryAuthorized) throw new Error($t("settings.directoryAccessUnconfirmed"));
      authMessage = $t(source.degraded ? "settings.directoryAuthorizedDegraded" : "settings.directoryAuthorized");
    });
  }

  async function openInAppLogin() {
    const targetPlatform = platform;
    await runAuth(async () => {
      await onOpenLogin(targetPlatform);
      authMessage = $t("settings.inAppLoginOpened");
    });
  }

  function requestConsent(platforms: PlatformId[]) {
    if (authLocked) return;
    authMessage = "";
    authError = false;
    if (!validProfile) {
      authMessage = $t("settings.selectProfile");
      authError = true;
      return;
    }
    consentTarget = { browserId, profileId, platforms: [...platforms] };
    consentOpen = true;
  }

  async function importCookies(consent: "once" | "always", allowElevation = false) {
    if (working || !consentTarget || !(allowElevation ? elevationOpen : consentOpen)) return;
    const target = { ...consentTarget, platforms: [...consentTarget.platforms] };
    const source = browserSources.find((item) => item.id === target.browserId);
    if (!source || source.degraded || !source.profiles.some((item) => item.id === target.profileId)) {
      authMessage = $t("settings.selectProfile");
      authError = true;
      consentOpen = false;
      elevationOpen = false;
      return;
    }
    authMessage = "";
    authError = false;
    pendingConsent = consent;
    try {
      if (target.platforms.length > 1) {
        const results = await onSyncBrowser({ ...target, consent });
        authMessage = results.map((result) => `${platformMeta[result.platform].label}: ${result.message}`).join("\n");
        authError = results.some((result) => result.status !== "active");
      } else {
        const result = await onImportBrowser(target.platforms[0], target.browserId, target.profileId, consent, allowElevation);
        if (result.requiresElevation) {
          consentOpen = false;
          elevationOpen = true;
          elevationConsent = consent;
          authMessage = result.message;
          return;
        }
        showResult(result);
      }
      consentOpen = false;
      elevationOpen = false;
    } catch (error) {
      authError = true;
      authMessage = resolveErrorMessage(error);
    } finally {
      pendingConsent = null;
    }
  }

  async function submitSettings() {
    if (authLocked) return;
    saveError = "";
    try {
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
    } catch (error) {
      saveError = resolveErrorMessage(error);
    }
  }

  async function saveManual() {
    const targetPlatform = platform;
    const value = manualCookie;
    await runAuth(async () => {
      const result = await onSaveManual(targetPlatform, value);
      if (result.status === "active") manualCookie = "";
      showResult(result);
    });
  }

  async function importCookieFile() {
    const targetPlatform = platform;
    await runAuth(async () => {
      const path = await onPickCookieFile();
      if (!path) return;
      showResult(await onImportCookieFile(targetPlatform, path));
    });
  }

  async function clearAuth() {
    const targetPlatform = platform;
    await runAuth(() => onClearAuth(targetPlatform));
  }
</script>

{#if open}
  <button class="sheet-backdrop" type="button" aria-label={$t("settings.close")} disabled={working} onclick={onClose}></button>
  <div class="settings-dialog" role="dialog" aria-label={$t("app.settings")} aria-modal="true">
    <aside class="settings-nav">
      <div class="settings-nav-head"><span class="eyebrow">{$t("settings.title")}</span></div>
      <button class:active={section === "auth"} type="button" disabled={authLocked} onclick={() => (section = "auth")}><KeyRound size={16} />{$t("settings.platformAuth")}</button>
      <button class:active={section === "download"} type="button" disabled={authLocked} onclick={() => (section = "download")}><Download size={16} />{$t("common.download")}</button>
      <button class:active={section === "appearance"} type="button" disabled={authLocked} onclick={() => (section = "appearance")}><Palette size={16} />{$t("settings.appearance")}</button>
      <button class:active={section === "about"} type="button" disabled={authLocked} onclick={() => (section = "about")}><Info size={16} />{$t("settings.about")}</button>
    </aside>

    <div class="settings-body">
      <header class="settings-body-head">
        <h2>{{ auth: $t("settings.platformAuth"), download: $t("common.download"), appearance: $t("settings.appearance"), about: $t("settings.about") }[section]}</h2>
        <button class="icon-button" type="button" title={$t("settings.close")} aria-label={$t("settings.close")} disabled={working} onclick={onClose}><X size={18} /></button>
      </header>

      <div class="settings-scroll">
        {#if section === "auth"}
          <section class="settings-section">
            <div class="platform-tabs" role="tablist" aria-label="认证平台">
              {#each Object.keys(platformMeta) as id}
                <button class:active={platform === id} type="button" role="tab" aria-selected={platform === id} disabled={authLocked} onclick={() => { if (authLocked) return; platform = id as PlatformId; restorePlatformBrowser(); authMessage = ""; authError = false; }}>{platformMeta[id as PlatformId].label}</button>
              {/each}
            </div>

            <div class="auth-status-line">
              <span class:active={bootstrap.platformAuth[platform].status === "active"}></span>
              <strong>{bootstrap.platformAuth[platform].status === "active" ? $t("auth.active") : $t("auth.guest")}</strong>
              {#if bootstrap.platformAuth[platform].browserId}<small>{bootstrap.platformAuth[platform].browserId}</small>{/if}
              {#if bootstrap.platformAuth[platform].status === "active"}
                <button class="icon-button" type="button" title={$t("settings.clearAuth")} aria-label={$t("settings.clearAuth")} disabled={authLocked} onclick={clearAuth}><Trash2 size={15} /></button>
              {/if}
            </div>

            <h3 class="auth-group-title">{$t("settings.autoFetch")}</h3>
            <p class="inline-message">{$t("settings.browserLoginHint")}</p>
            <label>{$t("settings.browserSource")}
              <Select value={browserId} disabled={authLocked} options={browserSources.map((source) => ({ value: source.id, label: source.label }))} onChange={(next) => { if (!authLocked) selectBrowser(next); }} />
            </label>
            {#if selectedBrowser()?.degraded}<p class="inline-message" role="status">{$t("settings.browserDegraded")}</p>{/if}
            {#if directoryAuthorization}
              <button class={selectedBrowser()?.degraded ? "primary-button" : "quiet-button directory-button"} type="button" disabled={authLocked} onclick={authorizeDirectory}><FolderOpen size={17} />{$t(selectedBrowser()?.directoryAuthorized ? "settings.reauthorizeDirectory" : "settings.authorizeDirectory")}</button>
              <p class="inline-message" role="status">{$t(selectedBrowser()?.directoryAuthorized ? "settings.directoryAccessGranted" : "settings.directoryAccessPending")}</p>
              <p class="inline-message">{$t("settings.directoryAccessHint")}</p>
            {/if}
            <label>Profile
              <Select value={profileId} disabled={authLocked} options={[{ value: "", label: $t("settings.selectProfile") }, ...profiles.filter((profile) => profile.id.trim()).map((profile) => ({ value: profile.id, label: profile.label }))]} onChange={(next) => { if (!authLocked) profileId = next; }} />
            </label>
            {#if !validProfile}<p class="inline-message" role="status">{$t("settings.selectProfile")}</p>{/if}
            <button class="primary-button" type="button" disabled={!browserId || authLocked} onclick={() => requestConsent(allPlatforms)}>{#if pendingConsent}<LoaderCircle class="spin" size={17} />{:else}<ShieldCheck size={17} />{/if}{$t("settings.syncAllPlatforms")}</button>
            <button class="quiet-button single-platform-button" type="button" disabled={!browserId || authLocked} onclick={() => requestConsent([platform])}>{$t("settings.detectCookie")} · {platformMeta[platform].label}</button>

            <details class="manual-auth">
              <summary class="auth-group-title">{$t("settings.otherMethods")}</summary>
              <h3 class="auth-group-title">{$t("settings.manualFetch")}</h3>
              <label>{$t("settings.cookieTextLabel")} / cookies.txt
                <textarea bind:value={manualCookie} disabled={authLocked} rows="4" spellcheck="false" placeholder={$t("settings.cookieTextPlaceholder")}></textarea>
              </label>
              <div class="manual-auth-actions">
                <button class="quiet-button" type="button" disabled={authLocked} onclick={importCookieFile}><FileText size={16} />{$t("settings.importCookieFile")}</button>
                <button class="quiet-button" type="button" disabled={!manualCookie.trim() || authLocked} onclick={saveManual}><Check size={16} />{$t("settings.saveCookieText")}</button>
              </div>
              {#if platform !== "youtube"}
                <button class="quiet-button login-button" type="button" disabled={authLocked} onclick={openInAppLogin}><ShieldCheck size={17} />{$t("settings.inAppLogin")}</button>
                <p class="inline-message">{$t("settings.inAppLoginHint")}</p>
              {/if}
            </details>
            {#if authMessage}<p class:error={authError} class="inline-message auth-message" aria-live="polite">{authMessage}</p>{/if}
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
              <label>{$t("settings.proxy")}<input bind:value={proxyUrl} placeholder={$t("settings.proxyPlaceholder")} /></label>
              <label>{$t("settings.speedLimit")}<input bind:value={speedLimit} placeholder="8M" /></label>
            </div>
            {#if proxyAutoMode && bootstrap.proxySource === "system" && bootstrap.effectiveProxyUrl}
              <p class="proxy-status">{$t("settings.proxyAutoSystem")} {bootstrap.effectiveProxyUrl}</p>
            {:else if proxyAutoMode && bootstrap.proxySource === "none"}
              <p class="proxy-status">{$t("settings.proxyAutoNone")}</p>
            {/if}
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
              <img class="about-logo" src={appIconUrl} alt="" />
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
        <footer class="sheet-footer">{#if saveError}<span class="save-error" role="alert">{saveError}</span>{/if}<button class="primary-button" type="button" disabled={authLocked} onclick={submitSettings}>{#if working}<LoaderCircle class="spin" size={17} />{:else}<Check size={17} />{/if}{$t("settings.save")}</button></footer>
      {/if}
    </div>
  </div>

  {#if consentOpen && consentTarget}
    <div class="consent-dialog" role="dialog" aria-modal="true" aria-label={$t("settings.cookieConsent")}>
      <ShieldCheck size={24} />
      <h3>{$t("settings.allowRead")} {consentTarget.platforms.map((id) => platformMeta[id].label).join(" / ")}</h3>
      <dl><dt>{$t("settings.browserSource")}</dt><dd>{selectedBrowser()?.label} / {selectedBrowser()?.profiles.find((item) => item.id === profileId)?.label}</dd><dt>{$t("settings.domainScope")}</dt><dd>{consentTarget.platforms.map((id) => domains[id]).join("; ")}</dd><dt>{$t("settings.storageLocation")}</dt><dd>{$t("settings.localOnly")}</dd></dl>
      <p>{$t("settings.cookieBoundary")}</p>
      {#if authError && authMessage}<p class="inline-message error" role="alert">{authMessage}</p>{/if}
      <div class="dialog-actions"><button class="quiet-button" type="button" disabled={working} onclick={() => (consentOpen = false)}>{$t("common.cancel")}</button><button class="quiet-button" type="button" disabled={working} onclick={() => importCookies("once")}>{#if pendingConsent === "once"}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t("settings.once")}{/if}</button><button class="primary-button" type="button" disabled={working} onclick={() => importCookies("always")}>{#if pendingConsent === "always"}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t(consentTarget.platforms.length > 1 ? "settings.alwaysSelected" : "settings.always")}<ChevronRight size={16} />{/if}</button></div>
    </div>
  {/if}

  {#if elevationOpen}
    <div class="consent-dialog" role="dialog" aria-modal="true" aria-label="提权 Cookie helper 确认">
      <ShieldCheck size={24} />
      <h3>{$t("settings.elevationTitle")}</h3>
      <p class="inline-message" class:error={authError} aria-live="assertive">{authMessage}</p><p>{$t("settings.elevationBoundary")}</p>
      <div class="dialog-actions"><button class="quiet-button" type="button" disabled={working} onclick={() => (elevationOpen = false)}>{$t("common.cancel")}</button><button class="primary-button" type="button" disabled={working} onclick={() => importCookies(elevationConsent, true)}>{#if pendingConsent}<LoaderCircle class="spin" size={16} />{$t("settings.detectingCookie")}{:else}{$t("settings.continueUac")}<ChevronRight size={16} />{/if}</button></div>
    </div>
  {/if}
{/if}

<style>
  .directory-button, .single-platform-button, .login-button { width: 100%; margin-top: 12px; }
  summary { cursor: pointer; }
  .auth-message { white-space: pre-line; }
</style>
