<script lang="ts" module>
  const cache = new Map<string, string>();
  const pending = new Map<string, Promise<string | null>>();
</script>

<script lang="ts">
  import { fetchThumbnail } from "../backend";
  import PlatformIcon from "./PlatformIcon.svelte";
  import type { PlatformId } from "../types";

  let {
    url = null,
    platform,
    title = "",
    iconSize = 18
  }: {
    url?: string | null;
    platform: PlatformId;
    title?: string;
    iconSize?: number;
  } = $props();

  let image = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    if (!url) {
      image = null;
      return;
    }
    failed = false;
    const cached = cache.get(url);
    if (cached) {
      image = cached;
      return;
    }
    let cancelled = false;
    let request = pending.get(url);
    if (!request) {
      request = fetchThumbnail(url)
        .then((dataUrl) => {
          cache.set(url, dataUrl);
          return dataUrl;
        })
        .catch(() => null)
        .finally(() => pending.delete(url));
      pending.set(url, request);
    }
    void request.then((dataUrl) => {
      if (cancelled) return;
      if (dataUrl) image = dataUrl;
      else failed = true;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

<span class="thumb" data-platform={platform}>
  {#if image && !failed}
    <img src={image} alt={title} loading="lazy" decoding="async" onerror={() => (failed = true)} />
  {:else}
    <PlatformIcon {platform} size={iconSize} />
  {/if}
</span>

<style>
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    color: var(--platform-accent, var(--faint));
    background: var(--input);
    aspect-ratio: 16 / 10;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
