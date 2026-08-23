<script lang="ts">
  import { Copy, Minus, Square, X } from "@lucide/svelte";
  import { isFramelessWindows } from "../backend";
  import { t } from "../i18n";

  const visible = isFramelessWindows();

  let maximized = $state(false);
  let appWindow: import("@tauri-apps/api/window").Window | null = null;

  if (visible) {
    void import("@tauri-apps/api/window").then((api) => {
      appWindow = api.getCurrentWindow();
      void appWindow.isMaximized().then((value) => (maximized = value));
      void appWindow.onResized(() => {
        void appWindow?.isMaximized().then((value) => (maximized = value));
      });
    });
  }
</script>

{#if visible}
  <div class="titlebar" data-tauri-drag-region>
    <div class="titlebar-controls">
      <button type="button" title={$t("window.minimize")} aria-label={$t("window.minimize")} onclick={() => appWindow?.minimize()}><Minus size={14} /></button>
      <button type="button" title={maximized ? $t("window.restore") : $t("window.maximize")} aria-label={maximized ? $t("window.restore") : $t("window.maximize")} onclick={() => appWindow?.toggleMaximize()}>{#if maximized}<Copy size={12} />{:else}<Square size={12} />{/if}</button>
      <button class="titlebar-close" type="button" title={$t("common.close")} aria-label={$t("common.close")} onclick={() => appWindow?.close()}><X size={15} /></button>
    </div>
  </div>
{/if}
