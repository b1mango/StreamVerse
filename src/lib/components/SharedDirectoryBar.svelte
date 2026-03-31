<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let currentDirectory = "";
  export let defaultDirectory = "";
  export let picking = false;
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    pick: void;
    reset: void;
    open: void;
  }>();
</script>

<div class="shared-strip">
  <div class="path-copy">
    <span>保存到</span>
    <strong title={currentDirectory}>{currentDirectory}</strong>
  </div>

  <div class="path-actions">
    <button class="ghost-button" onclick={() => dispatch("open")} disabled={disabled}>
      打开目录
    </button>
    <button
      class="ghost-button"
      onclick={() => dispatch("pick")}
      disabled={picking || disabled}
    >
      {picking ? "选择中…" : "更改目录"}
    </button>
    <button
      class="ghost-button"
      onclick={() => dispatch("reset")}
      disabled={currentDirectory === defaultDirectory || disabled}
    >
      恢复默认
    </button>
  </div>
</div>
