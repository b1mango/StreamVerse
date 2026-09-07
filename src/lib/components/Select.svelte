<script lang="ts">
  import { tick } from "svelte";
  import { Check, ChevronDown } from "@lucide/svelte";

  export type SelectOption = { value: string; label: string };

  let {
    value,
    options,
    onChange,
    disabled = false,
    ariaLabel
  }: {
    value: string;
    options: SelectOption[];
    onChange: (value: string) => void;
    disabled?: boolean;
    ariaLabel?: string;
  } = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let popup = $state<HTMLDivElement | null>(null);
  let popupStyle = $state("");

  const selected = $derived(options.find((option) => option.value === value));

  // 挂到 body 下：设置弹窗的 backdrop-filter 会让 position:fixed 以弹窗为包含块，导致坐标偏移
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      }
    };
  }

  async function openPopup() {
    if (!trigger || options.length === 0) return;
    const rect = trigger.getBoundingClientRect();
    const maxHeight = 264;
    const estimated = Math.min(options.length * 34 + 10, maxHeight);
    const spaceBelow = window.innerHeight - rect.bottom;
    const openUp = spaceBelow < estimated + 8 && rect.top > spaceBelow;
    popupStyle = openUp
      ? `left: ${rect.left}px; bottom: ${window.innerHeight - rect.top + 4}px; width: ${rect.width}px; max-height: ${maxHeight}px;`
      : `left: ${rect.left}px; top: ${rect.bottom + 4}px; width: ${rect.width}px; max-height: ${maxHeight}px;`;
    open = true;
    await tick();
    const target = popup?.querySelector<HTMLButtonElement>(".select-option.selected") ?? popup?.querySelector<HTMLButtonElement>(".select-option");
    target?.focus();
  }

  function choose(option: SelectOption) {
    onChange(option.value);
    open = false;
    trigger?.focus();
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      void openPopup();
    }
  }

  function onPopupKeydown(event: KeyboardEvent) {
    const items = Array.from(popup?.querySelectorAll<HTMLButtonElement>(".select-option") ?? []);
    const current = items.findIndex((item) => item === document.activeElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = event.key === "ArrowDown" ? Math.min(current + 1, items.length - 1) : Math.max(current - 1, 0);
      items[next]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      (event.key === "Home" ? items[0] : items[items.length - 1])?.focus();
    }
  }

  $effect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (trigger?.contains(target) || popup?.contains(target)) return;
      open = false;
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        open = false;
        trigger?.focus();
      }
    };
    const close = () => {
      open = false;
    };
    // 弹窗内部（选项列表）的滚动不应关闭弹窗，只有外部滚动才关
    const onScroll = (event: Event) => {
      if (popup && event.target instanceof Node && popup.contains(event.target)) return;
      open = false;
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", close);
    };
  });
</script>

<div class="select">
  <button
    bind:this={trigger}
    class="select-trigger"
    type="button"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={ariaLabel}
    {disabled}
    onclick={() => (open ? (open = false) : void openPopup())}
    onkeydown={onTriggerKeydown}
  >
    <span>{selected?.label ?? "—"}</span>
    <ChevronDown size={14} />
  </button>
  {#if open}
    <div bind:this={popup} use:portal class="select-popup" role="listbox" tabindex={-1} style={popupStyle} onkeydown={onPopupKeydown}>
      {#each options as option (option.value)}
        <button
          class="select-option"
          class:selected={option.value === value}
          type="button"
          role="option"
          aria-selected={option.value === value}
          onclick={() => choose(option)}
        >
          <span>{option.label}</span>
          <Check size={13} />
        </button>
      {/each}
    </div>
  {/if}
</div>
