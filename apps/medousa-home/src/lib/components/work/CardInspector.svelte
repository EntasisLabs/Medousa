<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { sendInteractiveTurn, startInteractiveStream } from "$lib/daemon";
  import { columnLabel } from "$lib/types/workspace";

  interface Props {
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onBack?: () => void;
    onClose?: () => void;
    split?: boolean;
  }

  let { onOpenNote, onOpenChat, onBack, onClose, split = false }: Props = $props();

  function handleClose() {
    if (onClose) {
      onClose();
      return;
    }
    onBack?.();
  }

  const detail = $derived(workspace.selectedCardDetail);
  const wrappingUp = $derived(detail?.card.column === "wrapping_up");

  async function askMedousa() {
    if (!detail) return;
    const prompt = `Tell me about work card ${detail.card.id}: "${detail.card.title}". Status: ${detail.card.status_label}.`;
    chat.beginUserMessage(prompt);
    onOpenChat();
    try {
      const accepted = await sendInteractiveTurn(chat.sessionId, prompt);
      await startInteractiveStream(accepted.stream_url);
    } catch (err) {
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }
</script>

<section
  class="flex h-full w-full min-w-0 flex-col border-l border-surface-500/20 bg-surface-950/80"
>
  <header class="flex items-center justify-between gap-3 border-b border-surface-500/20 px-4 py-3">
    <div class="min-w-0">
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface mb-2"
        onclick={handleClose}
      >
        {split ? "Close" : "← Board"}
      </button>
      <h1 class="truncate text-base font-semibold">
        {detail?.card.title ?? "Card inspector"}
      </h1>
      {#if detail}
        <p class="text-xs text-surface-400">
          {detail.card.id} · {columnLabel(detail.card.column)} · {detail.kind}
        </p>
      {/if}
    </div>
    <div class="flex shrink-0 flex-wrap gap-2">
      <button
        type="button"
        class="btn btn-sm variant-filled-warning"
        disabled={!detail || detail.terminal}
        onclick={() => workspace.cancelSelectedCard()}
      >
        Cancel
      </button>
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={!detail}
        onclick={() => workspace.retrySelectedCard()}
      >
        Retry
      </button>
      <button
        type="button"
        class="btn btn-sm variant-soft-primary"
        disabled={!detail}
        onclick={askMedousa}
      >
        Ask Medousa
      </button>
    </div>
  </header>

  {#if workspace.cardDetailError}
    <p class="border-b border-error-500/30 bg-error-500/10 px-4 py-2 text-xs text-error-300">
      {workspace.cardDetailError}
    </p>
  {/if}

  {#if workspace.cardActionMessage}
    <p class="border-b border-primary-500/20 bg-primary-500/10 px-4 py-2 text-xs text-primary-200">
      {workspace.cardActionMessage}
    </p>
  {/if}

  {#if !detail}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      Select a card from the board or work rail.
    </div>
  {:else}
    <div class="flex-1 space-y-4 overflow-y-auto px-5 py-4 text-sm">
      <div class="grid gap-3 sm:grid-cols-2">
        <div class="rounded-container-token bg-surface-800/60 p-3">
          <p class="text-xs text-surface-400">Status</p>
          <p class="mt-1 font-medium {wrappingUp ? 'text-warning-300' : ''}">
            {detail.card.status_label}
          </p>
        </div>
        <div class="rounded-container-token bg-surface-800/60 p-3">
          <p class="text-xs text-surface-400">Column</p>
          <p class="mt-1 font-medium capitalize">
            {columnLabel(detail.card.column)}
          </p>
        </div>
        {#if detail.subtitle}
          <div class="rounded-container-token bg-surface-800/60 p-3">
            <p class="text-xs text-surface-400">Subtitle</p>
            <p class="mt-1">{detail.subtitle}</p>
          </div>
        {/if}
        {#if detail.manuscript_id}
          <div class="rounded-container-token bg-surface-800/60 p-3">
            <p class="text-xs text-surface-400">Manuscript</p>
            <p class="mt-1">{detail.manuscript_id}</p>
          </div>
        {/if}
        {#if detail.session_id}
          <div class="rounded-container-token bg-surface-800/60 p-3">
            <p class="text-xs text-surface-400">Session</p>
            <p class="mt-1 truncate">{detail.session_id}</p>
          </div>
        {/if}
        {#if detail.job_id}
          <div class="rounded-container-token bg-surface-800/60 p-3">
            <p class="text-xs text-surface-400">Job</p>
            <p class="mt-1 truncate">{detail.job_id}</p>
          </div>
        {/if}
      </div>

      {#if detail.wrapping_up_reasons.length > 0}
        <div
          class="rounded-container-token border border-warning-500/40 bg-warning-500/10 p-4 {wrappingUp
            ? 'animate-pulse'
            : ''}"
        >
          <p class="text-xs font-semibold uppercase tracking-wide text-warning-300">
            Wrapping up
          </p>
          <ul class="mt-2 list-disc space-y-1 pl-4 text-warning-100">
            {#each detail.wrapping_up_reasons as reason (reason)}
              <li>{reason}</li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if detail.result_excerpt}
        <div class="rounded-container-token bg-surface-800/60 p-4">
          <p class="text-xs text-surface-400">Result</p>
          <pre
            class="mt-2 whitespace-pre-wrap font-mono text-xs leading-relaxed text-surface-100"
          >{detail.result_excerpt}</pre>
        </div>
      {/if}

      {#if detail.error}
        <div class="rounded-container-token border border-error-500/30 bg-error-500/10 p-4">
          <p class="text-xs text-error-300">Error</p>
          <p class="mt-2 text-error-100">{detail.error}</p>
        </div>
      {/if}

      {#if detail.associations.vault_paths.length > 0}
        <div class="rounded-container-token bg-surface-800/60 p-4">
          <p class="text-xs text-surface-400">Linked vault notes</p>
          <ul class="mt-2 space-y-1">
            {#each detail.associations.vault_paths as path (path)}
              <li>
                <button
                  type="button"
                  class="text-left text-primary-400 hover:underline"
                  onclick={() => onOpenNote(path)}
                >
                  {path}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}
</section>
