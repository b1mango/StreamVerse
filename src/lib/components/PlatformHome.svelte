<script lang="ts">
  import { createEventDispatcher } from "svelte";
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
    <h2>选择功能</h2>
  </div>

  <div class="platform-grid">
    {#each moduleOrder as moduleId}
      {@const meta = moduleCatalog[moduleId]}
      {@const state = moduleState(moduleId)}
      <article class={`platform-card ${meta.accent} ${state.installed ? "" : "is-disabled"}`}>
        <div class="platform-card-head">
          <span class="chip subtle">{meta.badge}</span>
          <span class={`chip ${state.installed ? "accent" : "subtle"}`}>
            {state.installed ? "已内置" : "当前构建未包含"}
          </span>
        </div>

        <div class="platform-card-copy">
          <strong>{meta.label}</strong>
          <p>{meta.description}</p>
        </div>

        <div class="module-tags">
          {#each meta.dependencyHints as item}
            <span class="mini-tag">{item}</span>
          {/each}
          {#if state.currentVersion}
            <span class="mini-tag">版本 {state.currentVersion}</span>
          {:else if state.latestVersion}
            <span class="mini-tag">版本 {state.latestVersion}</span>
          {/if}
        </div>

        <div class="module-actions">
          <button
            class="primary-button"
            disabled={!state.installed}
            onclick={() => openModule(moduleId)}
            type="button"
          >
            打开
          </button>
        </div>
      </article>
    {/each}
  </div>
</section>
