<script lang="ts">
  import { CheckCircle2, CircleX, FolderOpen, Pause, Play, RefreshCw, Trash2, X } from "@lucide/svelte";
  import { platformMeta } from "../options";
  import { t } from "../i18n";
  import type { DownloadTask } from "../types";

  let {
    tasks,
    collapsed = false,
    onControl,
    onReveal,
    onRemove,
    onClear
  }: {
    tasks: DownloadTask[];
    collapsed?: boolean;
    onControl: (task: DownloadTask, action: "pause" | "resume" | "cancel" | "retry") => void;
    onReveal: (task: DownloadTask) => void;
    onRemove: (task: DownloadTask) => void;
    onClear: () => void;
  } = $props();
</script>

<aside class:collapsed class="task-panel" aria-label={$t("app.taskQueue")}>
  <header class="task-header">
    <div><span class="eyebrow">LIVE QUEUE</span><strong>{tasks.length.toString().padStart(2, "0")}</strong></div>
    <button class="icon-button" type="button" title={$t("task.clearFinished")} aria-label={$t("task.clearFinished")} onclick={onClear}><Trash2 size={17} /></button>
  </header>
  <div class="task-list" aria-live="polite">
    {#each tasks as task (task.id)}
      <article class:failed={task.status === "failed"} class:completed={task.status === "completed"} class="task-row">
        <div class="task-topline">
          <span class="platform-code" style:--platform-color={platformMeta[task.platform].color}>{platformMeta[task.platform].code}</span>
          <strong title={task.title}>{task.title}</strong>
          <span>{task.progress}%</span>
        </div>
        <div class="progress-track"><i style:width={`${task.progress}%`}></i></div>
        <div class="task-meta">
          <span>{$t(`task.${task.status}`)} · {task.speedText}</span><span>{task.etaText}</span>
        </div>
        {#if task.message}<p>{task.message}</p>{/if}
        <div class="task-actions">
          {#if task.status === "downloading" && task.supportsPause}
            <button class="icon-button" type="button" title={$t("common.pause")} aria-label={`${$t("common.pause")} ${task.title}`} onclick={() => onControl(task, "pause")}><Pause size={15} /></button>
          {:else if task.status === "paused"}
            <button class="icon-button" type="button" title={$t("common.resume")} aria-label={`${$t("common.resume")} ${task.title}`} onclick={() => onControl(task, "resume")}><Play size={15} /></button>
          {/if}
          {#if task.supportsCancel && ["queued", "downloading", "paused"].includes(task.status)}
            <button class="icon-button" type="button" title={$t("common.cancel")} aria-label={`${$t("common.cancel")} ${task.title}`} onclick={() => onControl(task, "cancel")}><X size={15} /></button>
          {/if}
          {#if task.canRetry && ["failed", "cancelled"].includes(task.status)}
            <button class="icon-button" type="button" title={$t("common.retry")} aria-label={`${$t("common.retry")} ${task.title}`} onclick={() => onControl(task, "retry")}><RefreshCw size={15} /></button>
          {/if}
          {#if task.outputPath}
            <button class="icon-button" type="button" title={$t("task.revealFile")} aria-label={`${$t("task.revealFile")} ${task.title}`} onclick={() => onReveal(task)}><FolderOpen size={15} /></button>
          {/if}
          {#if ["completed", "failed", "cancelled"].includes(task.status)}
            <button class="icon-button task-remove" type="button" title="移除任务" aria-label={`移除 ${task.title}`} onclick={() => onRemove(task)}>
              {#if task.status === "completed"}<CheckCircle2 size={15} />{:else}<CircleX size={15} />{/if}
            </button>
          {/if}
        </div>
      </article>
    {:else}
      <div class="queue-empty"><span>QUEUE IDLE</span><p>{$t("task.empty")}</p></div>
    {/each}
  </div>
</aside>
