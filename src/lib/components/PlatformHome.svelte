<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { t } from "../i18n";
  import type { ModuleId, ModuleRuntimeState } from "../types";
  import { moduleCatalog, moduleOrder } from "../options";

  export let modules: ModuleRuntimeState[] = [];

  const dispatch = createEventDispatcher<{
    open: { moduleId: ModuleId };
  }>();

  function moduleState(moduleId: ModuleId) {
    return (
      modules.find((module) => module.id === moduleId) ?? {
        id: moduleId,
        installed: true,
        enabled: true,
        updateAvailable: false
      }
    );
  }

  function openModule(moduleId: ModuleId) {
    const state = moduleState(moduleId);
    if (!state.installed) {
      return;
    }

    dispatch("open", { moduleId });
  }
</script>

<section class="platform-home panel">
  <div class="hero-copy">
    <p class="eyebrow">Built-in Platforms</p>
    <h2>{$t("home.title")}</h2>
  </div>

  <div class="platform-grid">
    {#each moduleOrder as moduleId}
      {@const meta = moduleCatalog[moduleId]}
      {@const state = moduleState(moduleId)}
      <article class={`platform-card ${meta.accent} ${state.installed ? "" : "is-disabled"}`}>
        <div class="platform-card-head">
          <span class="chip subtle">{meta.badge}</span>
          <span class={`chip ${state.installed ? "accent" : "subtle"}`}>
            {state.installed ? $t("home.installed") : $t("home.notInstalled")}
          </span>
        </div>

        <div class="platform-card-copy">
          <strong>{$t("module." + moduleId + ".label")}</strong>
          <p>{$t("module." + moduleId + ".description")}</p>
        </div>

        <div class="module-tags">
          {#each meta.dependencyHints as item}
            <span class="mini-tag">{item}</span>
          {/each}
          {#if state.currentVersion}
            <span class="mini-tag">{$t("home.version")} {state.currentVersion}</span>
          {:else if state.latestVersion}
            <span class="mini-tag">{$t("home.version")} {state.latestVersion}</span>
          {/if}
        </div>

        <div class="module-actions">
          <button
            class="primary-button"
            disabled={!state.installed}
            onclick={() => openModule(moduleId)}
            type="button"
          >
            {$t("home.open")}
          </button>
        </div>
      </article>
    {/each}
  </div>
</section>
