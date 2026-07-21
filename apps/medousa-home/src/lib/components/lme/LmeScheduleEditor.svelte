<script lang="ts">
  import ScheduleDetailEditor from "$lib/components/automations/ScheduleDetailEditor.svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  const active = $derived(
    lmeWorkspace.activeTab?.kind === "schedule" ? lmeWorkspace.activeTab : null,
  );

  const entry = $derived(
    active
      ? (automations.definitions.find((row) => row.recurring_id === active.recurringId) ??
        null)
      : null,
  );

  $effect(() => {
    if (!active) return;
    void automations.refresh();
    void automations.loadRuns(active.recurringId);
  });

  async function handleDeleted() {
    if (!active) return;
    await lmeWorkspace.closeTab(active.tabId);
  }
</script>

<div class="lme-schedule-editor flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
  {#if !active}
    <p class="px-5 py-5 text-sm text-surface-500 sm:px-7 sm:py-6">
      Select a schedule from the side panel.
    </p>
  {:else if !entry}
    <div class="px-5 py-5 sm:px-7 sm:py-6">
      {#if automations.loading}
        <p class="text-sm text-surface-500">Loading schedule…</p>
      {:else}
        <p class="text-sm text-surface-500">
          Schedule
          <span class="font-mono text-surface-300">{active.recurringId}</span>
          not found.
        </p>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface mt-3 self-start"
          onclick={() => void automations.refresh()}
        >
          Refresh
        </button>
      {/if}
    </div>
  {:else}
    <ScheduleDetailEditor {entry} onDeleted={() => void handleDeleted()} />
  {/if}
</div>
