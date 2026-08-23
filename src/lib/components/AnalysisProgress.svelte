<script lang="ts">
  import { Activity } from "@lucide/svelte";
  import type { AnalysisProgress as Progress } from "../types";

  let { progress }: { progress: Progress } = $props();
  let determinate = $derived(progress.total > 0);
  let percent = $derived(
    determinate ? Math.min(100, Math.round((progress.current / progress.total) * 100)) : 0
  );
</script>

<section class="analysis-progress" aria-live="polite" aria-label="批量解析进度">
  <div class="analysis-progress-copy">
    <Activity size={16} />
    <strong>{progress.message}</strong>
    <span>{determinate ? `${progress.current} / ${progress.total}` : "正在建立作品索引"}</span>
  </div>
  <div
    class:indeterminate={!determinate}
    class="analysis-progress-track"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={determinate ? percent : undefined}
  >
    <i style:width={determinate ? `${percent}%` : "32%"}></i>
  </div>
</section>
