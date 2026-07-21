<script lang="ts">
  import { CirclePause, CirclePlay, Trash2 } from "@lucide/svelte";
  import ScheduleDeliveriesPopover from "$lib/components/automations/ScheduleDeliveriesPopover.svelte";
  import ScheduleRunsPopover from "$lib/components/automations/ScheduleRunsPopover.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import type { RecurringDefinitionEntry } from "$lib/types/recurring";

  interface Props {
    entry: RecurringDefinitionEntry;
    hideSidebarExpand?: boolean;
    onDeleted?: () => void;
  }

  let {
    entry,
    hideSidebarExpand = false,
    onDeleted,
  }: Props = $props();

  let runsOpen = $state(false);
  let deliveriesOpen = $state(false);
  let confirmDelete = $state(false);

  function closeOthers(except: "runs" | "deliveries") {
    if (except !== "runs") runsOpen = false;
    if (except !== "deliveries") deliveriesOpen = false;
  }

  async function toggleEnabled() {
    await automations.setEnabled(entry.recurring_id, !entry.enabled);
  }

  async function confirmRemove() {
    await automations.remove(entry.recurring_id);
    confirmDelete = false;
    onDeleted?.();
  }
</script>

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
    <ScheduleRunsPopover
      bind:open={runsOpen}
      recurringId={entry.recurring_id}
      onOpenChange={(next) => {
        if (next) {
          closeOthers("runs");
          confirmDelete = false;
        }
      }}
    />
    <ScheduleDeliveriesPopover
      bind:open={deliveriesOpen}
      {entry}
      onOpenChange={(next) => {
        if (next) {
          closeOthers("deliveries");
          confirmDelete = false;
        }
      }}
    />

    <span class="mx-0.5 h-4 w-px shrink-0 bg-surface-500/40" aria-hidden="true"></span>

    <button
      type="button"
      class="scripts-workbench-toolbar-btn"
      title={entry.enabled ? "Pause" : "Resume"}
      aria-label={entry.enabled ? "Pause schedule" : "Resume schedule"}
      disabled={automations.updatingId === entry.recurring_id}
      onclick={() => void toggleEnabled()}
    >
      {#if entry.enabled}
        <CirclePause size={15} strokeWidth={1.75} />
      {:else}
        <CirclePlay size={15} strokeWidth={1.75} />
      {/if}
    </button>

    {#if confirmDelete}
      <button
        type="button"
        class="scripts-workbench-toolbar-btn text-error-400 hover:text-error-300"
        title="Confirm delete"
        aria-label="Confirm delete schedule"
        disabled={automations.deletingId === entry.recurring_id}
        onclick={() => void confirmRemove()}
      >
        <Trash2 size={15} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="px-1.5 text-[10px] text-surface-500 hover:text-surface-300"
        onclick={() => (confirmDelete = false)}
      >
        Cancel
      </button>
    {:else}
      <button
        type="button"
        class="scripts-workbench-toolbar-btn"
        title="Delete"
        aria-label="Delete schedule"
        onclick={() => {
          runsOpen = false;
          deliveriesOpen = false;
          confirmDelete = true;
        }}
      >
        <Trash2 size={15} strokeWidth={1.75} />
      </button>
    {/if}
  </div>
</div>
