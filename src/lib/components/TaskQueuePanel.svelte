<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { t } from "../i18n";
  import type { DownloadTask } from "../types";
  import { finishedTaskCount } from "../media";
  import { taskLabelMap } from "../options";

  export let tasks: DownloadTask[] = [];
  export let pendingTaskIds: string[] = [];
  export let clearingFinished = false;

  const dispatch = createEventDispatcher<{
    pause: { task: DownloadTask };
    resume: { task: DownloadTask };
    cancel: { task: DownloadTask };
    retry: { task: DownloadTask };
    reveal: { task: DownloadTask };
    clearFinished: void;
  }>();

  function pendingTaskAction(taskId: string) {
    return pendingTaskIds.includes(taskId);
  }
</script>

<article class="panel tasks-panel">
  <div class="section-head">
    <div>
      <p class="eyebrow">Recent Tasks</p>
      <h3>{$t("task.recentTasks")}</h3>
    </div>

    <div class="section-actions">
      {#if finishedTaskCount(tasks) > 0}
        <button
          class="text-button"
          onclick={() => dispatch("clearFinished")}
          disabled={clearingFinished}
        >
          {clearingFinished ? $t("task.clearing") : $t("task.clearFinished")}
        </button>
      {/if}
      <span class="chip subtle">{tasks.length} {$t("common.unit")}</span>
    </div>
  </div>

  {#if tasks.length === 0}
    <p class="empty-state">{$t("task.empty")}</p>
  {:else}
    <div class="task-list">
      {#each tasks as task (task.id)}
        <div class="task-row">
          <div class="task-copy">
            <strong>{task.title}</strong>
            <span>{task.formatLabel}</span>
            <div class="task-progress">
              <div
                class:completed={task.status === "completed"}
                class:failed={task.status === "failed" || task.status === "cancelled"}
                class="task-progress-fill"
                style={`width: ${task.progress}%`}
              ></div>
            </div>
            {#if task.message}
              <small>{task.message}</small>
            {/if}
          </div>

          <div class="task-side">
            <span>{task.progress}%</span>
            <strong>{$t("task." + task.status)}</strong>
            {#if task.status !== "completed" && task.status !== "failed" && task.status !== "cancelled"}
              <small>{task.speedText} · ETA {task.etaText}</small>
            {/if}

            <div class="task-actions">
              {#if task.status === "downloading" && task.supportsPause}
                <button
                  class="text-button"
                  onclick={() => dispatch("pause", { task })}
                  disabled={pendingTaskAction(task.id)}
                  type="button"
                >
                  {$t("common.pause")}
                </button>
              {/if}

              {#if task.status === "paused" && task.supportsPause}
                <button
                  class="text-button"
                  onclick={() => dispatch("resume", { task })}
                  disabled={pendingTaskAction(task.id)}
                  type="button"
                >
                  {$t("common.resume")}
                </button>
              {/if}

              {#if ["queued", "downloading", "paused"].includes(task.status) && task.supportsCancel}
                <button
                  class="text-button danger"
                  onclick={() => dispatch("cancel", { task })}
                  disabled={pendingTaskAction(task.id)}
                  type="button"
                >
                  {$t("common.cancel")}
                </button>
              {/if}

              {#if ["failed", "cancelled"].includes(task.status) && task.canRetry}
                <button
                  class="text-button"
                  onclick={() => dispatch("retry", { task })}
                  disabled={pendingTaskAction(task.id)}
                  type="button"
                >
                  {$t("common.retry")}
                </button>
              {/if}

              {#if task.outputPath}
                <button
                  class="text-button"
                  onclick={() => dispatch("reveal", { task })}
                  type="button"
                >
                  {$t("task.revealFile")}
                </button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</article>
