<script lang="ts">
  import { archiveAskJob, completeAskJobActions } from "$lib/daemon";
  import { loadProductConfigSummary } from "$lib/messaging";
  import type { ProductConfigSummary } from "$lib/types/messaging";
  import {
    defaultJournalPathForToday,
    type PendingAskCompletion,
  } from "$lib/types/askJob";
  import { workspace } from "$lib/stores/workspace.svelte";

  interface Props {
    pending: PendingAskCompletion | null;
    onOpenNote: (path: string) => void;
    onClose: () => void;
  }

  let { pending, onOpenNote, onClose }: Props = $props();

  let journalPath = $state(defaultJournalPathForToday());
  let writeJournal = $state(true);
  let notifyChannel = $state<string>("");
  let submitting = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let config = $state<ProductConfigSummary | null>(null);

  const notifyOptions = $derived.by(() => {
    const options: { id: string; label: string }[] = [{ id: "", label: "Don't notify" }];
    if (!config) return options;
    if (config.telegram.heartbeatChatIds.length > 0) {
      options.push({ id: "telegram", label: "Telegram" });
    }
    if (config.discord.heartbeatChannelIds.length > 0) {
      options.push({ id: "discord", label: "Discord" });
    }
    if (config.slack.heartbeatChannelIds.length > 0) {
      options.push({ id: "slack", label: "Slack" });
    }
    if (config.whatsapp.heartbeatChatJids.length > 0) {
      options.push({ id: "whatsapp", label: "WhatsApp" });
    }
    return options;
  });

  $effect(() => {
    if (pending) {
      journalPath = defaultJournalPathForToday();
      writeJournal = true;
      notifyChannel = "";
      message = null;
      error = null;
      void loadProductConfigSummary().then((summary) => {
        config = summary;
      });
    }
  });

  async function saveAndClose(removeFromBoard: boolean) {
    if (!pending || submitting) return;
    submitting = true;
    error = null;
    message = null;
    try {
      const response = await completeAskJobActions(pending.jobId, {
        writeJournalPath: writeJournal ? journalPath.trim() : undefined,
        notifyChannel: notifyChannel || undefined,
      });
      message = response.message;
      if (response.journal_path) {
        onOpenNote(response.journal_path);
      }
      if (removeFromBoard) {
        await archiveAskJob(pending.jobId, true);
      }
      workspace.clearPendingAskCompletion();
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  async function dismissOnly() {
    workspace.clearPendingAskCompletion();
    onClose();
  }
</script>

{#if pending}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center bg-surface-950/70 p-4 sm:items-center"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) void dismissOnly();
    }}
  >
    <div
      class="w-full max-w-lg rounded-container-token border border-surface-600/40 bg-surface-900 p-5 shadow-xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ask-completion-title"
    >
      <h2 id="ask-completion-title" class="text-base font-semibold text-surface-50">
        Ask finished
      </h2>
      <p class="mt-1 text-sm text-surface-300">{pending.title}</p>
      <p class="mt-2 text-xs text-surface-500">
        Result is saved on this machine. Choose where to send it, or dismiss and read it in Work.
      </p>

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
            <span class="mt-1 block text-xs text-surface-500">
              Set heartbeat chat IDs in Messaging to enable channel notify.
            </span>
          {/if}
        </label>
      </div>

      {#if error}
        <p class="mt-3 text-xs text-error-400">{error}</p>
      {:else if message}
        <p class="mt-3 text-xs text-primary-300">{message}</p>
      {/if}

      <div class="mt-5 flex flex-wrap justify-end gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={submitting}
          onclick={() => void dismissOnly()}
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
  </div>
{/if}
