<script lang="ts">
  import { Sparkles } from "@lucide/svelte";
  import { onMount } from "svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    open?: boolean;
    goal?: string;
    mobile?: boolean;
    onOpenChange?: (open: boolean) => void;
  }

  let {
    open = $bindable(false),
    goal = $bindable(""),
    mobile = false,
    onOpenChange,
  }: Props = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let triggerEl: HTMLButtonElement | undefined = $state();
  let panelStyle = $state("");

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
    title="Plan with AI"
    aria-label="Plan with AI"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <Sparkles size={15} strokeWidth={1.75} />
  </button>

  {#if open}
    <div
      bind:this={menuEl}
      class="agent-editor-popover-panel agent-editor-popover-panel-schedule"
      style={panelStyle}
      role="dialog"
      aria-label="Plan with AI"
    >
      <div class="agent-editor-popover-head">
        <div class="min-w-0">
          <p class="text-[11px] font-semibold tracking-[-0.01em] text-surface-100">Plan with AI</p>
          <p class="mt-0.5 text-[10px] text-surface-500">Generate steps from a goal</p>
        </div>
      </div>

      <div class="agent-editor-popover-body overflow-y-auto p-3">
        <label class="block">
          <span class="flow-liquid-whisper">Goal</span>
          <div class="mt-1">
            <GrowingTextarea
              bind:value={goal}
              placeholder="Every weekday, search for news and summarize it for me…"
              minHeight={mobile ? 48 : 56}
              maxHeight={mobile ? 140 : 160}
              aria-label="Flow goal for planning"
            />
          </div>
        </label>
        <button
          type="button"
          class="btn btn-sm variant-soft-primary mt-3"
          disabled={flows.planning || !goal.trim()}
          onclick={() => void flows.planFromGoal(goal)}
        >
          {flows.planning ? "Planning…" : "Plan steps"}
        </button>
        {#if flows.lastPlan?.notes.length}
          <ul class="workshop-faint mt-3 list-disc space-y-1 pl-4 text-[11px]">
            {#each flows.lastPlan.notes.slice(0, 3) as note (note)}
              <li>{note}</li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/if}
</div>
