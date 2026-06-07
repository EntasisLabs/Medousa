<script lang="ts">
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    getJobResult,
    lookupArtifact,
    sendInteractiveTurn,
    startInteractiveStream,
  } from "$lib/daemon";
  import type { ArtifactPreview } from "$lib/types/artifact";
  import type { JobResultResponse } from "$lib/types/job";
  import { columnLabel } from "$lib/types/workspace";
  import { formatCardTitle, formatStatusLabel } from "$lib/utils/formatWork";
  import { findBlockedGroupForCard } from "$lib/utils/groupWork";
  import {
    filterCardTimeline,
    formatWorkspaceEventKind,
  } from "$lib/utils/cardTimeline";
  import { formatToolName } from "$lib/utils/formatTurn";

  interface Props {
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
    onBack?: () => void;
    onClose?: () => void;
    split?: boolean;
  }

  let { onOpenNote, onOpenChat, onBack, onClose, split = false }: Props = $props();

  let jobResult = $state<JobResultResponse | null>(null);
  let jobResultError = $state<string | null>(null);
  let jobResultLoading = $state(false);
  let artifactPreviews = $state<ArtifactPreview[]>([]);
  let artifactsLoading = $state(false);

  function handleClose() {
    if (onClose) {
      onClose();
      return;
    }
    onBack?.();
  }

  const detail = $derived(workspace.selectedCardDetail);
  const wrappingUp = $derived(detail?.card.column === "wrapping_up");
  const blockedGroup = $derived(
    detail ? findBlockedGroupForCard(workspace.cards, detail.card.id) : null,
  );
  const timeline = $derived(
    detail ? filterCardTimeline(workspace.feed, detail.card.id) : [],
  );

  $effect(() => {
    const artifactIds = detail?.associations.artifact_ids ?? [];
    const sessionId = detail?.session_id;
    if (!artifactIds.length || !sessionId) {
      artifactPreviews = [];
      return;
    }

    artifactsLoading = true;
    void Promise.all(
      artifactIds.map(async (artifactId) => {
        try {
          const response = await lookupArtifact(sessionId, artifactId);
          return {
            artifact_id: artifactId,
            rendered_output: response.rendered_output,
          } satisfies ArtifactPreview;
        } catch (err) {
          return {
            artifact_id: artifactId,
            rendered_output: "",
            error: err instanceof Error ? err.message : String(err),
          } satisfies ArtifactPreview;
        }
      }),
    )
      .then((previews) => {
        artifactPreviews = previews;
      })
      .finally(() => {
        artifactsLoading = false;
      });
  });

  $effect(() => {
    const jobId = detail?.job_id;
    if (!jobId) {
      jobResult = null;
      jobResultError = null;
      return;
    }

    jobResultLoading = true;
    jobResultError = null;
    void getJobResult(jobId)
      .then((result) => {
        jobResult = result;
      })
      .catch((err) => {
        jobResultError = err instanceof Error ? err.message : String(err);
        jobResult = null;
      })
      .finally(() => {
        jobResultLoading = false;
      });
  });

  async function askMedousa() {
    if (!detail) return;
    const prompt = `Tell me about work card ${detail.card.id}: "${detail.card.title}". Status: ${detail.card.status_label}.`;
    chat.beginUserMessage(prompt);
    onOpenChat();
    try {
      const accepted = await sendInteractiveTurn(chat.sessionId, prompt, {
        provider: runtime.provider,
        model: runtime.model,
        responseDepthMode: runtime.depthMode,
        stageRouting: runtime.stageRouting,
      });
      await startInteractiveStream(accepted.stream_url);
    } catch (err) {
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }

  function formatTimestamp(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString();
  }
</script>

<section
  class="workshop-rail flex h-full w-full min-w-0 flex-col"
>
  <header class="workshop-header flex items-center justify-between gap-3 py-3">
    <div class="min-w-0">
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface mb-2"
        onclick={handleClose}
      >
        {split ? "Close" : "← Board"}
      </button>
      <h1 class="truncate text-base font-semibold">
        {detail ? formatCardTitle(detail.card) : "Card inspector"}
      </h1>
      {#if detail}
        <p class="workshop-faint">
          {detail.card.id} · {columnLabel(detail.card.column)} · {detail.kind}
        </p>
      {/if}
    </div>
    <div class="flex shrink-0 flex-wrap gap-2">
      {#if blockedGroup && blockedGroup.cards.length > 1}
        <button
          type="button"
          class="btn btn-sm variant-soft-primary"
          onclick={() => workspace.retryBlockedGroup(blockedGroup)}
        >
          Retry all ×{blockedGroup.cards.length}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => workspace.dismissBlockedGroup(blockedGroup)}
        >
          Dismiss all
        </button>
      {/if}
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
    <div class="workshop-muted flex flex-1 items-center justify-center">
      Select a card from the board or work rail.
    </div>
  {:else}
    <div class="flex-1 space-y-4 overflow-y-auto px-5 py-4 text-sm">
      <div class="grid gap-3 sm:grid-cols-2">
        <div class="workshop-inset p-3">
          <p class="workshop-label">Status</p>
          <p class="mt-1 font-medium {wrappingUp ? 'text-warning-300' : ''}">
            {formatStatusLabel(detail.card.status_label)}
          </p>
        </div>
        <div class="workshop-inset p-3">
          <p class="workshop-label">Column</p>
          <p class="mt-1 font-medium capitalize">
            {columnLabel(detail.card.column)}
          </p>
        </div>
        {#if detail.subtitle}
          <div class="workshop-inset p-3">
            <p class="workshop-label">Subtitle</p>
            <p class="mt-1">{detail.subtitle}</p>
          </div>
        {/if}
        {#if detail.manuscript_id}
          <div class="workshop-inset p-3">
            <p class="workshop-label">Manuscript</p>
            <p class="mt-1">{detail.manuscript_id}</p>
          </div>
        {/if}
        {#if detail.session_id}
          <div class="workshop-inset p-3">
            <p class="workshop-label">Session</p>
            <p class="mt-1 truncate">{detail.session_id}</p>
          </div>
        {/if}
        {#if detail.job_id}
          <div class="workshop-inset p-3">
            <p class="workshop-label">Job</p>
            <p class="mt-1 truncate">{detail.job_id}</p>
          </div>
        {/if}
        {#if detail.work_id}
          <div class="workshop-inset p-3">
            <p class="workshop-label">Worker</p>
            <p class="mt-1 truncate">{detail.work_id}</p>
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

      {#if timeline.length > 0 || detail.tool_names?.length}
        <div class="workshop-inset p-4">
          <p class="workshop-label">Worker timeline</p>
          {#if detail.tool_names?.length}
            <p class="mt-2 font-mono text-[10px] text-surface-500">
              {detail.tool_names.map((tool) => formatToolName(tool)).join(" · ")}
            </p>
          {/if}
          {#if timeline.length > 0}
            <ul class="mt-3 space-y-2">
              {#each timeline as event (event.id)}
                <li class="border-l border-surface-500/30 pl-3">
                  <p class="text-xs text-surface-300">{event.summary}</p>
                  <p class="workshop-faint mt-0.5">
                    {formatWorkspaceEventKind(event.kind)} ·
                    {formatTimestamp(event.timestamp_utc)}
                  </p>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      {#if detail.job_id}
        <div class="workshop-inset p-4">
          <div class="flex items-center justify-between gap-3">
            <p class="workshop-label">Job result</p>
            {#if jobResult}
              <span class="workshop-faint font-mono">
                {jobResult.status}{#if jobResult.is_terminal} · terminal{/if}
              </span>
            {/if}
          </div>
          {#if jobResultLoading}
            <p class="workshop-faint mt-2">Loading job output…</p>
          {:else if jobResultError}
            <p class="mt-2 text-xs text-warning-400">{jobResultError}</p>
          {:else if jobResult?.output_text}
            <pre
              class="mt-2 max-h-80 overflow-y-auto whitespace-pre-wrap font-mono text-xs leading-relaxed text-surface-100"
            >{jobResult.output_text}</pre>
          {:else if jobResult}
            <p class="workshop-faint mt-2">
              No output yet
              {#if jobResult.latest_outcome}
                · {jobResult.latest_outcome}
              {/if}
            </p>
          {/if}
        </div>
      {:else if detail.result_excerpt}
        <div class="workshop-inset p-4">
          <p class="workshop-label">Result</p>
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

      {#if detail.associations.artifact_ids.length > 0}
        <div class="workshop-inset p-4">
          <p class="workshop-label">
            Artifacts · {detail.associations.artifact_ids.length}
          </p>
          {#if artifactsLoading}
            <p class="workshop-faint mt-2">Loading artifact previews…</p>
          {:else}
            <ul class="mt-3 space-y-3">
              {#each artifactPreviews as preview (preview.artifact_id)}
                <li class="rounded-container-token border border-surface-500/15 p-3">
                  <p class="font-mono text-[11px] text-primary-300">{preview.artifact_id}</p>
                  {#if preview.error}
                    <p class="mt-1 text-xs text-warning-400">{preview.error}</p>
                  {:else}
                    <pre
                      class="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-surface-300"
                    >{preview.rendered_output}</pre>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      {#if detail.associations.vault_paths.length > 0}
        <div class="workshop-inset p-4">
          <p class="workshop-label">Linked vault notes</p>
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
