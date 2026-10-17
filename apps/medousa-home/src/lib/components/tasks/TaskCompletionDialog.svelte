<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import TaskDialogFrame from "./TaskDialogFrame.svelte";
  import { archiveAskJob, completeAskJobActions } from "$lib/daemon";
  import { loadProductConfigSummary } from "$lib/messaging";
  import type { ProductConfigSummary } from "$lib/types/messaging";
  import { defaultJournalPathForToday, type PendingAskCompletion } from "$lib/types/askJob";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { openTaskDetails, openVaultNote } from "$lib/runtime/shellUseCases";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";

  let { pending }: { pending: PendingAskCompletion } = $props();
  const jobId = untrack(() => pending.jobId);
  const scope = activeWorkshopId();
  let alive = true;
  onDestroy(() => { alive = false; });
  const current = () => alive && activeWorkshopId() === scope && workspace.pendingAskCompletion?.jobId === jobId;
  let journalPath = $state(defaultJournalPathForToday());
  let writeJournal = $state(true);
  let notifyChannel = $state("");
  let submitting = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let config = $state<ProductConfigSummary | null>(null);
  let configError = $state(false);

  const notifyOptions = $derived.by(() => {
    const options = [{ id: "", label: "Don't notify" }];
    if (config?.telegram.heartbeatChatIds.length) options.push({ id: "telegram", label: "Telegram" });
    if (config?.discord.heartbeatChannelIds.length) options.push({ id: "discord", label: "Discord" });
    if (config?.slack.heartbeatChannelIds.length) options.push({ id: "slack", label: "Slack" });
    if (config?.whatsapp.heartbeatChatJids.length) options.push({ id: "whatsapp", label: "WhatsApp" });
    return options;
  });
  onMount(() => {
    void loadProductConfigSummary().then((summary) => { if (current()) config = summary; })
      .catch(() => { if (current()) configError = true; });
  });

  async function saveAndClose(removeFromBoard: boolean) {
    if (!current() || submitting) return;
    submitting = true; error = null; message = null;
    try {
      const response = await completeAskJobActions(jobId, {
        writeJournalPath: writeJournal ? journalPath.trim() : undefined,
        notifyChannel: notifyChannel || undefined,
      });
      if (!current()) return;
      if (!response.ok) throw new Error(response.message);
      if (removeFromBoard) {
        const archived = await archiveAskJob(jobId, true);
        if (!current()) return;
        if (!archived.archived) throw new Error(archived.message);
      }
      workspace.clearPendingAskCompletion(jobId);
      if (response.journal_path) await openVaultNote(response.journal_path);
    } catch (cause) { if (current()) error = cause instanceof Error ? cause.message : String(cause); }
    finally { if (current()) submitting = false; }
  }
  function dismissOnly() { if (current()) workspace.clearPendingAskCompletion(jobId); }
  function viewTask() {
    if (!current()) return;
    void openTaskDetails(jobId);
    workspace.clearPendingAskCompletion(jobId);
  }
</script>

<TaskDialogFrame label="Task finished" compact mobile={layout.isMobile} onClose={dismissOnly}>
  <div class="p-5">
      <h2 id="ask-completion-title" class="text-base font-semibold text-surface-50">
        Ask finished
      </h2>
      <p class="mt-1 text-sm text-content-secondary">{pending.title}</p>
      <p class="mt-2 text-xs text-content-quiet">
        The result is saved in your workshop. Open task details to read it, or choose where to send it.
      </p>

      {#if configError}<p class="mt-3 text-xs text-content-warning">Delivery options are unavailable. You can still save to your journal.</p>{/if}
      <div class="mt-4 space-y-3">
        <label class="flex items-start gap-2 text-sm text-surface-200">
          <input type="checkbox" class="checkbox mt-0.5" bind:checked={writeJournal} />
          <span>
            Write to journal
            <input
              class="input mt-1.5 w-full text-xs"
              bind:value={journalPath}
              disabled={!writeJournal || submitting}
              placeholder="journal/2026-05-30.md"
            />
          </span>
        </label>

        <label class="block text-sm text-surface-200">
          Notify via
          <select
            class="select mt-1.5 w-full text-sm"
            bind:value={notifyChannel}
            disabled={submitting || notifyOptions.length <= 1}
          >
            {#each notifyOptions as option (option.id)}
              <option value={option.id}>{option.label}</option>
            {/each}
          </select>
          {#if notifyOptions.length <= 1}
            <span class="mt-1 block text-xs text-content-quiet">
              Set heartbeat chat IDs in Messaging to enable channel notify.
            </span>
          {/if}
        </label>
      </div>

      {#if error}
        <p class="mt-3 text-xs text-content-error">{error}</p>
      {:else if message}
        <p class="mt-3 text-xs text-content-link">{message}</p>
      {/if}

      <div class="mt-5 flex flex-wrap justify-end gap-2">
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={submitting} onclick={viewTask}>View task</button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={submitting}
          onclick={dismissOnly}
        >
          Later
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-primary"
          disabled={submitting}
          onclick={() => void saveAndClose(false)}
        >
          {submitting ? "…" : "Save"}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={submitting}
          onclick={() => void saveAndClose(true)}
        >
          {submitting ? "…" : "Save & clear"}
        </button>
      </div>

  </div>
</TaskDialogFrame>
