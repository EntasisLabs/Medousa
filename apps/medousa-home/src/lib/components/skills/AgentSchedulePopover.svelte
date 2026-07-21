<script lang="ts">
  import { CalendarClock } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { ManuscriptScheduledToolEntry } from "$lib/types/manuscript";

  interface Props {
    open?: boolean;
    scheduleReady: boolean;
    scheduleErrorHuman: string;
    scheduleCron?: string;
    scheduleExecutionMode?: string;
    deliveryMode?: string;
    deliveryOnComplete?: string;
    scheduledTools?: ManuscriptScheduledToolEntry[];
    onOpenChange?: (open: boolean) => void;
    onChooseTools?: () => void;
    onScheduleSkill?: () => void;
    onUseInAutomation?: () => void;
  }

  let {
    open = $bindable(false),
    scheduleReady,
    scheduleErrorHuman,
    scheduleCron = $bindable(""),
    scheduleExecutionMode = $bindable("agent_turn"),
    deliveryMode = $bindable(""),
    deliveryOnComplete = $bindable(""),
    scheduledTools = [],
    onOpenChange,
    onChooseTools,
    onScheduleSkill,
    onUseInAutomation,
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

  const hasCron = $derived(scheduleCron.trim().length > 0);
  const needsAttention = $derived(!scheduleReady && (hasCron || !!scheduleErrorHuman));

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
    {#if needsAttention}
      <span class="agent-editor-popover-dot agent-editor-popover-dot-warn" aria-hidden="true"></span>
    {:else if hasCron}
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
        <div class="rounded-lg border border-surface-500/25 bg-surface-950/30 px-3 py-2.5 text-xs">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-surface-400">Schedule readiness</span>
            {#if scheduleReady}
              <span class="text-[10px] tracking-wide text-primary-300 uppercase">Ready</span>
            {:else}
              <span class="text-[10px] tracking-wide text-warning-400 uppercase">Needs a step</span>
            {/if}
          </div>
          {#if scheduleErrorHuman}
            <p class="mt-1 text-warning-400/90">{scheduleErrorHuman}</p>
            {#if /tool/i.test(scheduleErrorHuman) && onChooseTools}
              <button
                type="button"
                class="workshop-text-action mt-2 text-xs"
                onclick={() => {
                  setOpen(false);
                  onChooseTools();
                }}
              >
                Choose tools →
              </button>
            {/if}
          {/if}
        </div>

        <div class="mt-4 grid gap-4">
          <label class="block">
            <span class="agent-liquid-whisper">Default schedule</span>
            <input
              class="agent-liquid-input mt-1 font-mono"
              bind:value={scheduleCron}
              placeholder="0 9 * * *"
              aria-label="Default schedule cron"
            />
          </label>
          <label class="block">
            <span class="agent-liquid-whisper">How it runs</span>
            <select
              class="agent-liquid-input mt-1"
              bind:value={scheduleExecutionMode}
              aria-label="How it runs"
            >
              <option value="agent_turn">Full agent turn</option>
              <option value="prompt">Quick prompt only</option>
            </select>
          </label>
          <label class="block">
            <span class="agent-liquid-whisper">Delivery</span>
            <input
              class="agent-liquid-input mt-1"
              bind:value={deliveryMode}
              placeholder="optional"
              aria-label="Delivery"
            />
          </label>
          <label class="block">
            <span class="agent-liquid-whisper">On complete</span>
            <select
              class="agent-liquid-input mt-1"
              bind:value={deliveryOnComplete}
              aria-label="On complete"
            >
              <option value="">None</option>
              <option value="locus">Remember (Locus)</option>
              <option value="store">Store</option>
            </select>
          </label>
        </div>

        {#if scheduledTools.length > 0}
          <div class="mt-4">
            <p class="agent-liquid-whisper">Scheduled lane preview</p>
            <ul class="mt-2 max-h-36 space-y-1 overflow-y-auto">
              {#each scheduledTools as row (row.tool)}
                <li class="rounded-lg border border-surface-500/25 px-2.5 py-1.5 text-[11px]">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="font-mono text-surface-200">{row.tool}</span>
                    <span
                      class="text-[10px] tracking-wide uppercase {row.allowed_on_schedule
                        ? 'text-primary-300'
                        : 'text-warning-400/80'}"
                    >
                      {row.allowed_on_schedule ? "schedule ok" : "interactive only"}
                    </span>
                  </div>
                  {#if row.reason}
                    <p class="workshop-faint mt-0.5">{row.reason}</p>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <div class="mt-4 flex flex-wrap gap-3">
          {#if onScheduleSkill}
            <button
              type="button"
              class="workshop-text-action text-xs"
              onclick={onScheduleSkill}
            >
              Schedule…
            </button>
          {/if}
          {#if onUseInAutomation}
            <button
              type="button"
              class="workshop-text-action text-xs"
              onclick={onUseInAutomation}
            >
              Use in automation
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
