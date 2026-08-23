<script lang="ts">
  import { siBilibili, siTiktok, siYoutube, type SimpleIcon } from "simple-icons";
  import type { PlatformId } from "../types";

  const icons: Record<PlatformId, SimpleIcon> = {
    douyin: siTiktok,
    bilibili: siBilibili,
    youtube: siYoutube
  };

  let { platform, size = 16 }: { platform: PlatformId; size?: number } = $props();
  let icon = $derived(icons[platform]);
</script>

<svg
  class:platform-icon-douyin={platform === "douyin"}
  class:platform-icon-bilibili={platform === "bilibili"}
  class:platform-icon-youtube={platform === "youtube"}
  class="platform-icon"
  viewBox="0 0 24 24"
  width={size}
  height={size}
  aria-hidden="true"
  focusable="false"
>
  <path d={icon.path}></path>
</svg>
