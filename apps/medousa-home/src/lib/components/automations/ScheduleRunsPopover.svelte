<script lang="ts">
  import { History } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import type { RecurringRunEntry } from "$lib/types/recurring";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    open?: boolean;
    recurringId: string;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    recurringId,
    onOpenChange,
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  const runs = $derived(automations.runsById[recurringId] ?? []);
  const failStreak = $derived.by(() => {
    let n = 0;
    for (const run of runs) {
      if (run.status === "failed") n += 1;
      else break;
    }
    return n;
  });

  function placePanel() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const width = Math.min(22 * 16, window.innerWidth - 24);
    let left = rect.right - width;
    left = Math.max(12, Math.min(left, window.innerWidth - width - 12));
    const top = Math.min(rect.bottom + 6, window.innerHeight - 48);
    panelStyle = `top:${top}px;left:${left}px;width:${width}px;`;
  }

  function setOpen(next: boolean) {
    open = next;
    onOpenChange?.(next);
    if (next) {
      placePanel();
      void automations.loadRuns(recurringId);
    }
  }

  function toggleOpen(event: MouseEvent) {
    event.stopPropagation();
    setOpen(!open);
  }

  function excerptFor(run: RecurringRunEntry): string | null {
    const outcome = run.latest_outcome?.trim();
    if (outcome) return automations.clipForList(outcome, 90);
    const out = run.output_text?.trim();
    if (!out) return null;
    return automations.clipForList(out.replace(/\s+/g, " "), 90);
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
    title="Runs"
    aria-label="Open runs"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <History size={15} strokeWidth={1.75} />
    {#if runs.length > 0}
      <span class="toolbar-dm-badge" aria-hidden="true">
        {runs.length > 9 ? "9+" : runs.length}
      </span>
    {/if}
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-schedule"
      style={panelStyle}
      role="dialog"
      aria-label="Runs"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Runs</p>
          <p class="mt-0.5 text-[10px] text-surface-500">
            {#if failStreak >= 2}
              Last {failStreak} failed
            {:else}
              Recent memory
            {/if}
          </p>
        </div>
      </div>
      <div class="agent-editor-popover-body max-h-[min(22rem,60vh)] overflow-y-auto">
        {#if automations.runsLoadingId === recurringId}
          <p class="schedule-popover-empty">Loading…</p>
        {:else if automations.runsErrorById[recurringId]}
          <p class="schedule-popover-empty schedule-popover-empty-warn">
            {automations.runsErrorById[recurringId]}
          </p>
        {:else if runs.length === 0}
          <p class="schedule-popover-empty">Nothing yet — the next tick shows up here.</p>
        {:else}
          <ul class="schedule-popover-list">
            {#each runs as run (run.job_id)}
              {@const excerpt = excerptFor(run)}
              {@const failed = run.status === "failed"}
              <li class="schedule-popover-row" title={run.job_id}>
                <div class="schedule-popover-row-top">
                  <p class="schedule-popover-when">
                    {automations.formatTimestamp(run.updated_at_utc)}
                  </p>
                  <p
                    class="schedule-popover-status"
                    class:schedule-popover-status-warn={failed}
                  >
                    {automations.statusLabel(run.status)}
                  </p>
                </div>
                {#if excerpt}
                  <p class="schedule-popover-excerpt">{excerpt}</p>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}
</div>
