<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { PlatformId } from "../types";
  import { platformMeta } from "../options";

  const dispatch = createEventDispatcher<{ select: { platform: PlatformId } }>();

  function openPlatform(platform: PlatformId) {
    dispatch("select", { platform });
  }
</script>

<section class="platform-home panel">
  <div class="hero-copy">
    <p class="eyebrow">Platform Hub</p>
    <h2>先选择平台，再进入对应下载工作区</h2>
    <p class="lede">
      入口页只做一件事：让用户明确当前正在处理的是哪一个平台，避免把抖音、Bilibili 和 YouTube 的规则混在一个页面里。
    </p>
  </div>

  <div class="platform-grid">
    {#each Object.entries(platformMeta) as [platform, meta]}
      <button
        class={`platform-card ${meta.accent}`}
        onclick={() => openPlatform(platform as PlatformId)}
        type="button"
      >
        <div class="platform-card-head">
          <span class="chip subtle">{meta.badge}</span>
          <span class="chip subtle">{meta.status}</span>
        </div>

        <div class="platform-card-copy">
          <strong>{meta.label}</strong>
          <p>{meta.description}</p>
        </div>
      </button>
    {/each}
  </div>
</section>
