<script lang="ts">
  import { Send } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import type { RecurringDefinitionEntry } from "$lib/types/recurring";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    open?: boolean;
    entry: RecurringDefinitionEntry;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    entry,
    onOpenChange,
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  const destination = $derived(automations.deliveryLabelFor(entry));
  const destinationNote = $derived.by(() => {
    const label = destination.toLowerCase();
    if (label.includes("telegram")) return "Sent as a Telegram message.";
    if (label.includes("quiet")) return "Runs without a delivery.";
    return "Results stay in Medousa.";
  });

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(18 * 16, window.innerWidth - 24);
    let left = rect.right - width;
    left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
    const top = Math.min(rect.bottom + 6, window.innerHeight - 48);
    panelStyle = `top:${top}px;left:${left}px;width:${width}px;`;
  }

  function setOpen(next: boolean) {
    open = next;
    onOpenChange?.(next);
    if (next) placePanel();
  }

  function toggleOpen(event: MouseEvent) {
    event.stopPropagation();
    setOpen(!open);
  }

  onMount(() => {
    const onDocPointerDown = (event: PointerEvent) => {
      if (!open) return;
      const path = event.composedPath();
      if (
        (menuEl && path.includes(menuEl)) ||
        (triggerEl && path.includes(triggerEl))
      ) {
        return;
      }
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) setOpen(false);
    };
    document.addEventListener("pointerdown", onDocPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDocPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  });

  $effect(() => {
    if (!open) return;
    placePanel();
    const onResize = () => placePanel();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  });
</script>

<div class="agent-editor-popover relative shrink-0">
  <button
    bind:this={triggerEl}
    type="button"
    class="scripts-workbench-toolbar-btn {open ? 'scripts-workbench-toolbar-btn-active' : ''}"
    title="Deliveries"
    aria-label="Open deliveries"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <Send size={15} strokeWidth={1.75} />
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-schedule"
      style={panelStyle}
      role="dialog"
      aria-label="Deliveries"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">
            Deliveries
          </p>
          <p class="mt-0.5 text-[10px] text-surface-500">Where results go</p>
        </div>
      </div>
      <div class="agent-editor-popover-body p-3">
        <p class="text-[13px] font-medium tracking-tight text-surface-100">{destination}</p>
        <p class="mt-1 text-[11px] leading-relaxed text-surface-500">{destinationNote}</p>
      </div>
    </div>
  {/if}
</div>
