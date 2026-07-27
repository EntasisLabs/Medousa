<script lang="ts">
  import { OctagonX, Play } from "@lucide/svelte";
  import FlowAddStepPopover from "$lib/components/automations/FlowAddStepPopover.svelte";
  import FlowPlanPopover from "$lib/components/automations/FlowPlanPopover.svelte";
  import FlowSchedulePopover from "$lib/components/automations/FlowSchedulePopover.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import type { WorkflowStepSpec } from "$lib/types/workflow";

  interface Props {
    hideSidebarExpand?: boolean;
    mobile?: boolean;
    goal?: string;
    cronExpr?: string;
    timezone?: string;
    stepCount: number;
    onRun: () => void;
    onCancel: () => void;
    onConfirmSchedule: () => void | Promise<void>;
    onAddStep: (step: WorkflowStepSpec) => void;
  }

  let {
    hideSidebarExpand = false,
    mobile = false,
    goal = $bindable(""),
    cronExpr = $bindable(""),
    timezone = $bindable("UTC"),
    stepCount,
    onRun,
    onCancel,
    onConfirmSchedule,
    onAddStep,
  }: Props = $props();

  let planOpen = $state(false);
  let scheduleOpen = $state(false);
  let addOpen = $state(false);

  function closeOthers(except: "plan" | "schedule" | "add") {
    if (except !== "plan") planOpen = false;
    if (except !== "schedule") scheduleOpen = false;
    if (except !== "add") addOpen = false;
  }

  $effect(() => {
    if (!mobile) return;
    const onAdd = () => {
      closeOthers("add");
      addOpen = true;
    };
    const onPlan = () => {
      closeOthers("plan");
      planOpen = true;
    };
    const onSchedule = () => {
      closeOthers("schedule");
      scheduleOpen = true;
    };
    const onRunEvent = () => onRun();
    const onClose = () => onCancel();
    window.addEventListener("medousa-mobile-flow-add", onAdd);
    window.addEventListener("medousa-mobile-flow-plan", onPlan);
    window.addEventListener("medousa-mobile-flow-schedule", onSchedule);
    window.addEventListener("medousa-mobile-flow-run", onRunEvent);
    window.addEventListener("medousa-mobile-flow-close", onClose);
    return () => {
      window.removeEventListener("medousa-mobile-flow-add", onAdd);
      window.removeEventListener("medousa-mobile-flow-plan", onPlan);
      window.removeEventListener("medousa-mobile-flow-schedule", onSchedule);
      window.removeEventListener("medousa-mobile-flow-run", onRunEvent);
      window.removeEventListener("medousa-mobile-flow-close", onClose);
    };
  });
</script>

{#if mobile}
  <!-- Popover hosts only — actions live in MobileTopChrome. -->
  <div
    class="pointer-events-none fixed z-[70] flex h-10 items-center gap-0.5 opacity-0"
    style="top: max(0.5rem, env(safe-area-inset-top, 0px)); right: 1rem;"
    aria-hidden="true"
  >
    <div class="pointer-events-auto">
      <FlowAddStepPopover
        bind:open={addOpen}
        {mobile}
        onAdd={onAddStep}
        onOpenChange={(next) => {
          if (next) closeOthers("add");
        }}
      />
    </div>
    <div class="pointer-events-auto">
      <FlowPlanPopover
        bind:open={planOpen}
        bind:goal
        {mobile}
        onOpenChange={(next) => {
          if (next) closeOthers("plan");
        }}
      />
    </div>
    <div class="pointer-events-auto">
      <FlowSchedulePopover
        bind:open={scheduleOpen}
        bind:cronExpr
        bind:timezone
        {mobile}
        {stepCount}
        onConfirm={onConfirmSchedule}
        onOpenChange={(next) => {
          if (next) closeOthers("schedule");
        }}
      />
    </div>
  </div>
{:else}
  <div
    class="scripts-workbench-titlebar relative z-40 flex shrink-0 items-center gap-1 border-b border-surface-500/35 px-1 py-0.5"
  >
    {#if !hideSidebarExpand}
      <ShellSidebarExpandButton label="Show workspace browser" />
    {/if}

    <div class="min-w-0 flex-1" aria-hidden="true"></div>

    <div
      class="scripts-workbench-titlebar-actions ml-auto flex shrink-0 items-center gap-0.5 pl-1"
    >
      <FlowAddStepPopover
        bind:open={addOpen}
        {mobile}
        onAdd={onAddStep}
        onOpenChange={(next) => {
          if (next) closeOthers("add");
        }}
      />

      <FlowPlanPopover
        bind:open={planOpen}
        bind:goal
        {mobile}
        onOpenChange={(next) => {
          if (next) closeOthers("plan");
        }}
      />

      <button
        type="button"
        class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-run"
        title={flows.running ? "Running…" : "Run"}
        aria-label="Run flow"
        disabled={flows.running || stepCount === 0}
        onclick={onRun}
      >
        <Play size={15} strokeWidth={1.75} />
      </button>

      <FlowSchedulePopover
        bind:open={scheduleOpen}
        bind:cronExpr
        bind:timezone
        {mobile}
        {stepCount}
        onConfirm={onConfirmSchedule}
        onOpenChange={(next) => {
          if (next) closeOthers("schedule");
        }}
      />

      <span class="mx-0.5 h-4 w-px shrink-0 bg-surface-500/40" aria-hidden="true"></span>

      <button
        type="button"
        class="scripts-workbench-toolbar-btn"
        title="Discard draft"
        aria-label="Discard draft"
        onclick={onCancel}
      >
        <OctagonX size={15} strokeWidth={1.75} />
      </button>
    </div>
  </div>
{/if}
