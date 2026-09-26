<script lang="ts">
  /** Answer-first transcript for a peer or workshop agent. */
  import { Bot, Check, ChevronDown, LoaderCircle, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { renderMarkdown } from "$lib/markdown";
  import { workerTranscripts } from "$lib/work/workerTranscripts.svelte";
  import { toolRunsFromWorkerActivity } from "$lib/utils/subagentRows";

  interface Props {
    workId: string | null;
    onClose: () => void;
  }

  let { workId, onClose }: Props = $props();

  const transcript = $derived(workId ? workerTranscripts.transcriptFor(workId) : null);
  const dispositionLabel = $derived(
    transcript?.disposition === "bound" ? "Workshop agent" : "Peer agent",
  );
  const runtimeLabel = $derived(
    executionTargets.runtimeLabel(transcript?.executionRuntimeId),
  );
  const toolRuns = $derived(toolRunsFromWorkerActivity(transcript?.toolRuns ?? []));
  const bodyText = $derived(transcript?.resultText?.trim() || transcript?.output?.trim() || "");
  const thinking = $derived(transcript?.thinking?.trim() ?? "");
  const thinkingHtml = $derived(renderMarkdown(thinking.replace(/\*{4}/g, "**\n\n**")));
  const isRunning = $derived(transcript?.streaming ?? false);
  const statusLabel = $derived(
    isRunning ? transcript?.statusLine?.trim() || "Working" : transcript?.statusLine?.trim() || "Complete",
  );

  /** Cold start (deep link / reload) still needs one fetch; SSE drives the rest. */
  $effect(() => {
    if (workId) void workerTranscripts.refresh(workId);
  });

  function handleClose() {
    haptic("light");
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!workId || event.key !== "Escape") return;
    event.preventDefault();
    handleClose();
  }

  $effect(() => {
    if (!workId) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!workId) return;
    return registerMobileBackHandler(() => {
      handleClose();
      return true;
    });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if workId}
  <BodyPortal>
    <div
      class="worker-transcript-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Agent transcript"
      tabindex="-1"
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
      onkeydown={handleKeydown}
    >
      <div class="worker-transcript-sheet">
        <header class="worker-transcript-header">
          <span class="worker-transcript-avatar" aria-hidden="true">
            <Bot size={17} strokeWidth={1.9} />
          </span>
          <div class="worker-transcript-heading">
            <div class="worker-transcript-eyebrow">
              <span>{dispositionLabel}</span>
              <span class="worker-transcript-status" class:worker-transcript-status-live={isRunning}>
                {#if isRunning}
                  <LoaderCircle class="h-3 w-3 animate-spin" strokeWidth={2.2} />
                {:else}
                  <Check class="h-3 w-3" strokeWidth={2.4} />
                {/if}
                {statusLabel}
              </span>
            </div>
            <h2 class="worker-transcript-title">{transcript?.title ?? workId}</h2>
          </div>
          <button
            type="button"
            class="worker-transcript-close"
            aria-label="Close transcript"
            onclick={handleClose}
          >
            <X size={17} strokeWidth={2.1} />
          </button>
        </header>

        <div class="worker-transcript-body">
          {#if !transcript}
            <div class="worker-transcript-loading">
              <LoaderCircle class="h-4 w-4 animate-spin" strokeWidth={2} />
              Loading transcript…
            </div>
          {:else}
            <div class="worker-transcript-layout">
              <main class="worker-transcript-main">
                <section class="worker-transcript-answer">
                  <p class="worker-transcript-label">{isRunning ? "Current output" : "Answer"}</p>
                  {#if bodyText}
                    <div class="worker-transcript-output markdown-body">
                      {@html renderMarkdown(bodyText)}
                    </div>
                  {:else if isRunning}
                    <div class="worker-transcript-working">
                      <span class="worker-transcript-pulse" aria-hidden="true"></span>
                      The agent is working. Its answer will appear here.
                    </div>
                  {:else}
                    <p class="worker-transcript-empty">No written answer was recorded.</p>
                  {/if}
                </section>

                {#if transcript.error}
                  <section class="worker-transcript-error">
                    <p class="worker-transcript-label">Error</p>
                    <p>{transcript.error}</p>
                  </section>
                {/if}

                {#if thinking}
                  <details class="worker-reasoning">
                    <summary>
                      <span>
                        <strong>Reasoning</strong>
                        <small>Inspect the agent’s working trace</small>
                      </span>
                      <span class="worker-reasoning-chevron" aria-hidden="true">
                        <ChevronDown size={15} strokeWidth={2} />
                      </span>
                    </summary>
                    <div class="worker-reasoning-copy markdown-body">
                      {@html thinkingHtml}
                    </div>
                  </details>
                {/if}
              </main>

              <aside class="worker-transcript-sidebar">
                <section class="worker-transcript-facts">
                  <p class="worker-transcript-label">Run details</p>
                  <dl>
                    <div>
                      <dt>Status</dt>
                      <dd>{statusLabel}</dd>
                    </div>
                    {#if runtimeLabel}
                      <div>
                        <dt>Runtime</dt>
                        <dd title={transcript.executionRuntimeId ?? undefined}>{runtimeLabel}</dd>
                      </div>
                    {/if}
                    {#if transcript.model}
                      <div>
                        <dt>Model</dt>
                        <dd>{transcript.model}</dd>
                      </div>
                    {/if}
                  </dl>
                </section>

                <section class="worker-transcript-activity">
                  <div class="worker-transcript-activity-heading">
                    <p class="worker-transcript-label">Activity</p>
                    <span>{toolRuns.length}</span>
                  </div>
                  {#if toolRuns.length > 0}
                    <ToolRunChips runs={toolRuns} compact inspectorCollapsed={false} />
                  {:else}
                    <p class="worker-transcript-empty">No tool activity.</p>
                  {/if}
                </section>
              </aside>
            </div>
          {/if}
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .worker-transcript-backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: max(1rem, env(safe-area-inset-top, 0px))
      max(1rem, env(safe-area-inset-right, 0px))
      max(1rem, env(safe-area-inset-bottom, 0px))
      max(1rem, env(safe-area-inset-left, 0px));
    background: color-mix(in srgb, rgb(var(--color-surface-950)) 78%, transparent);
    backdrop-filter: blur(8px);
    animation: worker-transcript-backdrop-in 180ms ease-out;
  }

  .worker-transcript-sheet {
    display: flex;
    width: min(54rem, 100%);
    height: min(46rem, calc(100dvh - 2rem));
    flex-direction: column;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, rgb(var(--color-surface-500)) 20%, transparent);
    border-radius: 1.15rem;
    background: rgb(var(--color-surface-950));
    color: rgb(var(--color-surface-100));
    box-shadow:
      0 32px 90px color-mix(in srgb, black 58%, transparent),
      inset 0 1px 0 color-mix(in srgb, white 5%, transparent);
    animation: worker-transcript-sheet-in 240ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  .worker-transcript-header {
    display: flex;
    flex-shrink: 0;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 1rem 1.05rem 0.9rem;
    border-bottom: 1px solid color-mix(in srgb, rgb(var(--color-surface-500)) 14%, transparent);
    background: linear-gradient(180deg, color-mix(in srgb, rgb(var(--color-primary-500)) 7%, transparent), transparent);
  }

  .worker-transcript-avatar {
    display: inline-flex;
    width: 2.15rem;
    height: 2.15rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 1px solid color-mix(in srgb, rgb(var(--color-primary-400)) 26%, transparent);
    border-radius: 0.65rem;
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 13%, transparent);
    color: rgb(var(--color-primary-300));
  }

  .worker-transcript-heading {
    min-width: 0;
    flex: 1;
  }

  .worker-transcript-eyebrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: rgb(var(--color-surface-400));
    font-size: 0.66rem;
    font-weight: 650;
  }

  .worker-transcript-status {
    display: inline-flex;
    align-items: center;
    gap: 0.22rem;
    border-radius: 999px;
    padding: 0.12rem 0.4rem;
    background: color-mix(in srgb, rgb(var(--color-success-500)) 11%, transparent);
    color: rgb(var(--color-success-300));
    font-size: 0.6rem;
  }

  .worker-transcript-status-live {
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 13%, transparent);
    color: rgb(var(--color-primary-300));
  }

  .worker-transcript-title {
    display: -webkit-box;
    margin: 0.25rem 0 0;
    overflow: hidden;
    color: rgb(var(--color-surface-50));
    font-size: 0.92rem;
    font-weight: 650;
    line-height: 1.35;
    letter-spacing: -0.01em;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .worker-transcript-close {
    display: inline-flex;
    width: 1.9rem;
    height: 1.9rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, rgb(var(--color-surface-500)) 12%, transparent);
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .worker-transcript-close:hover {
    background: color-mix(in srgb, rgb(var(--color-surface-500)) 22%, transparent);
    color: rgb(var(--color-surface-50));
  }

  .worker-transcript-body {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 1rem;
  }

  .worker-transcript-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(13rem, 17rem);
    align-items: start;
    gap: 1rem;
  }

  .worker-transcript-main,
  .worker-transcript-sidebar {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.8rem;
  }

  .worker-transcript-answer,
  .worker-transcript-facts,
  .worker-transcript-activity,
  .worker-reasoning,
  .worker-transcript-error {
    border: 1px solid color-mix(in srgb, rgb(var(--color-surface-500)) 15%, transparent);
    border-radius: 0.8rem;
    background: color-mix(in srgb, rgb(var(--color-surface-900)) 62%, transparent);
  }

  .worker-transcript-answer,
  .worker-transcript-facts,
  .worker-transcript-activity,
  .worker-transcript-error {
    padding: 0.85rem;
  }

  .worker-transcript-label {
    margin: 0 0 0.55rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  .worker-transcript-output {
    color: rgb(var(--color-surface-200));
    font-size: 0.82rem;
    line-height: 1.62;
  }

  .worker-transcript-output :global(p) {
    margin: 0 0 0.8em;
  }

  .worker-transcript-output :global(p:last-child) {
    margin-bottom: 0;
  }

  .worker-transcript-working,
  .worker-transcript-loading {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: rgb(var(--color-surface-400));
    font-size: 0.75rem;
  }

  .worker-transcript-loading {
    justify-content: center;
    min-height: 12rem;
  }

  .worker-transcript-pulse {
    width: 0.42rem;
    height: 0.42rem;
    border-radius: 999px;
    background: rgb(var(--color-primary-400));
    animation: worker-transcript-pulse 1.4s ease-in-out infinite;
  }

  .worker-transcript-facts dl {
    display: flex;
    margin: 0;
    flex-direction: column;
    gap: 0.5rem;
  }

  .worker-transcript-facts dl > div {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.8rem;
  }

  .worker-transcript-facts dt {
    color: rgb(var(--color-surface-500));
    font-size: 0.66rem;
  }

  .worker-transcript-facts dd {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    color: rgb(var(--color-surface-300));
    font-size: 0.67rem;
    font-weight: 550;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .worker-transcript-activity-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .worker-transcript-activity-heading > span {
    border-radius: 999px;
    padding: 0.08rem 0.38rem;
    background: color-mix(in srgb, rgb(var(--color-primary-500)) 12%, transparent);
    color: rgb(var(--color-primary-300));
    font-size: 0.6rem;
    font-variant-numeric: tabular-nums;
  }

  .worker-reasoning {
    overflow: hidden;
  }

  .worker-reasoning > summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.72rem 0.85rem;
    list-style: none;
    cursor: pointer;
  }

  .worker-reasoning > summary::-webkit-details-marker {
    display: none;
  }

  .worker-reasoning > summary span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.08rem;
  }

  .worker-reasoning > summary strong {
    color: rgb(var(--color-surface-300));
    font-size: 0.7rem;
    font-weight: 650;
  }

  .worker-reasoning > summary small {
    color: rgb(var(--color-surface-500));
    font-size: 0.62rem;
  }

  .worker-reasoning-chevron {
    display: inline-flex;
    flex-shrink: 0;
    color: rgb(var(--color-surface-500));
    transition: transform 180ms ease;
  }

  .worker-reasoning[open] .worker-reasoning-chevron {
    transform: rotate(180deg);
  }

  .worker-reasoning-copy {
    max-height: 20rem;
    overflow: auto;
    padding: 0.8rem 0.85rem 0.95rem;
    border-top: 1px solid color-mix(in srgb, rgb(var(--color-surface-500)) 12%, transparent);
    color: rgb(var(--color-surface-400));
    font-size: 0.7rem;
    line-height: 1.58;
  }

  .worker-reasoning-copy :global(p) {
    margin: 0 0 0.7em;
  }

  .worker-reasoning-copy :global(p:last-child) {
    margin-bottom: 0;
  }

  .worker-transcript-empty {
    margin: 0;
    color: rgb(var(--color-surface-500));
    font-size: 0.7rem;
    line-height: 1.5;
  }

  .worker-transcript-error {
    border-color: color-mix(in srgb, rgb(var(--color-error-400)) 22%, transparent);
    color: rgb(var(--color-error-300));
    font-size: 0.72rem;
  }

  .worker-transcript-error p:last-child {
    margin: 0;
  }

  @keyframes worker-transcript-backdrop-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes worker-transcript-sheet-in {
    from { opacity: 0; transform: translateY(12px) scale(0.985); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  @keyframes worker-transcript-pulse {
    0%, 100% { opacity: 0.35; transform: scale(0.85); }
    50% { opacity: 1; transform: scale(1); }
  }

  @media (max-width: 700px) {
    .worker-transcript-backdrop {
      align-items: flex-end;
      padding: env(safe-area-inset-top, 0px) 0 0;
    }

    .worker-transcript-sheet {
      width: 100%;
      height: min(88dvh, 48rem);
      border-right: 0;
      border-bottom: 0;
      border-left: 0;
      border-radius: 1.15rem 1.15rem 0 0;
    }

    .worker-transcript-layout {
      grid-template-columns: minmax(0, 1fr);
    }

    .worker-transcript-sidebar {
      order: 2;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .worker-transcript-backdrop,
    .worker-transcript-sheet,
    .worker-transcript-pulse {
      animation: none;
    }
  }
</style>
