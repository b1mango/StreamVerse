<script lang="ts">
  import { CheckCircle2, CircleX, FolderOpen, Pause, Play, RefreshCw, Trash2, X } from "@lucide/svelte";
  import { platformMeta } from "../options";
  import { t } from "../i18n";
  import Thumb from "./Thumb.svelte";
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

  const ACTIVE_STATUSES = ["queued", "downloading", "paused"];
  let activeTasks = $derived(tasks.filter((task) => ACTIVE_STATUSES.includes(task.status)));
  let finishedTasks = $derived(tasks.filter((task) => !ACTIVE_STATUSES.includes(task.status)));
</script>

{#snippet taskRow(task: DownloadTask)}
  <article class:failed={task.status === "failed"} class:completed={task.status === "completed"} class:paused={task.status === "paused"} class="task-row">
    <div class="task-main">
      <Thumb url={task.coverUrl} platform={task.platform} title={task.title} />
      <div class="task-copy">
        <strong title={task.title}>{task.title}</strong>
        <div class="task-meta">
          <span class="platform-code" style:--platform-color={platformMeta[task.platform].color}>{platformMeta[task.platform].code}</span>
          <span>{$t(`task.${task.status}`)}{task.status === "downloading" ? ` · ${task.speedText}` : ""}</span>
          {#if task.etaText && task.status === "downloading"}<span>{task.etaText}</span>{/if}
        </div>
      </div>
      <span class="task-percent">{task.progress}%</span>
      {#if task.message}<p class="task-error" class:task-note={task.status !== "failed"}>{task.message}</p>{/if}
    </div>
    <div class="progress-track"><i style:width={`${task.progress}%`}></i></div>
    <div class="task-actions">
      {#if task.status === "downloading" && task.supportsPause}
        <button class="icon-button" type="button" title={$t("common.pause")} aria-label={`${$t("common.pause")} ${task.title}`} onclick={() => onControl(task, "pause")}><Pause size={15} /></button>
      {:else if task.status === "paused"}
        <button class="icon-button" type="button" title={$t("common.resume")} aria-label={`${$t("common.resume")} ${task.title}`} onclick={() => onControl(task, "resume")}><Play size={15} /></button>
      {/if}
      {#if task.supportsCancel && ACTIVE_STATUSES.includes(task.status)}
        <button class="icon-button" type="button" title={$t("common.cancel")} aria-label={`${$t("common.cancel")} ${task.title}`} onclick={() => onControl(task, "cancel")}><X size={15} /></button>
      {/if}
      {#if task.canRetry && ["failed", "cancelled"].includes(task.status)}
        <button class="icon-button" type="button" title={$t("common.retry")} aria-label={`${$t("common.retry")} ${task.title}`} onclick={() => onControl(task, "retry")}><RefreshCw size={15} /></button>
      {/if}
      {#if task.outputPath}
        <button class="icon-button" type="button" title={$t("task.revealFile")} aria-label={`${$t("task.revealFile")} ${task.title}`} onclick={() => onReveal(task)}><FolderOpen size={15} /></button>
      {/if}
      {#if ["completed", "failed", "cancelled"].includes(task.status)}
        <button class="icon-button task-remove" type="button" title={$t("task.remove")} aria-label={`${$t("task.remove")} ${task.title}`} onclick={() => onRemove(task)}>
          {#if task.status === "completed"}<CheckCircle2 size={15} />{:else}<CircleX size={15} />{/if}
        </button>
      {/if}
    </div>
  </article>
{/snippet}

<aside class:collapsed class="task-panel" aria-label={$t("app.taskQueue")}>
  <header class="task-header">
    <div><h2>{$t("app.taskQueue")}</h2><span class="task-count">{tasks.length}</span></div>
    <button class="icon-button" type="button" title={$t("task.clearFinished")} aria-label={$t("task.clearFinished")} onclick={onClear}><Trash2 size={16} /></button>
  </header>
  <div class="task-list" aria-live="polite">
    {#if tasks.length === 0}
      <div class="queue-empty"><span>{$t("task.empty")}</span></div>
    {:else}
      {#if activeTasks.length > 0}
        <div class="task-group-label">{$t("task.groupActive")} · {activeTasks.length}</div>
        {#each activeTasks as task (task.id)}{@render taskRow(task)}{/each}
      {/if}
      {#if finishedTasks.length > 0}
        <div class="task-group-label">{$t("task.groupFinished")} · {finishedTasks.length}</div>
        {#each finishedTasks as task (task.id)}{@render taskRow(task)}{/each}
      {/if}
    {/if}
  </div>
</aside>
