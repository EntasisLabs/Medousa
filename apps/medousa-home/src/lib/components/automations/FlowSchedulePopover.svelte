<script lang="ts">
  import { CalendarClock } from "@lucide/svelte";
  import { onMount } from "svelte";
  import FriendlySchedulePicker from "$lib/components/automations/FriendlySchedulePicker.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    open?: boolean;
    cronExpr?: string;
    timezone?: string;
    mobile?: boolean;
    stepCount: number;
    onConfirm: () => void | Promise<void>;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    cronExpr = $bindable(""),
    timezone = $bindable("UTC"),
    mobile = false,
    stepCount,
    onConfirm,
    onOpenChange,
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  const hasCron = $derived(cronExpr.trim().length > 0);

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(24 * 16, window.innerWidth - 24);
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

  function toggleOpen() {
    setOpen(!open);
  }

  onMount(() => {
    const onDocClick = (event: MouseEvent) => {
      if (!open) return;
      const target = event.target as Node | null;
      if (menuEl?.contains(target) || triggerEl?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) setOpen(false);
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick);
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
    title="Schedule"
    aria-label="Open schedule"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <CalendarClock size={15} strokeWidth={1.75} />
    {#if hasCron}
      <span class="agent-editor-popover-dot" aria-hidden="true"></span>
    {/if}
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-schedule"
      style={panelStyle}
      role="dialog"
      aria-label="Schedule"
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Schedule</p>
          <p class="mt-0.5 text-[10px] text-surface-500">Optional recurring runs</p>
        </div>
      </div>

      <div class="agent-editor-popover-body overflow-y-auto p-3">
        <FriendlySchedulePicker
          {mobile}
          optional
          bind:cronExpr
          bind:timezone
          label="When it repeats"
        />
        <button
          type="button"
          class="btn btn-sm variant-soft-primary mt-3"
          disabled={flows.scheduling || stepCount === 0 || !cronExpr.trim()}
          onclick={() => void onConfirm()}
        >
          {flows.scheduling ? "Scheduling…" : "Schedule"}
        </button>
      </div>
    </div>
  {/if}
</div>
