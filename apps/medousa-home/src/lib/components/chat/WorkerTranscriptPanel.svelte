<script lang="ts">
  /**
   * Summoned sub-agent transcript. A peek, not a place: it overlays the thread
   * on demand and dismisses back to chat, so the host conversation stays primary.
   */
  import { Bot, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { renderMarkdown } from "$lib/markdown";
  import { workerTranscripts } from "$lib/stores/workerTranscripts.svelte";
  import { toolRunsFromWorkerActivity } from "$lib/utils/subagentRows";

  interface Props {
    workId: string | null;
    onClose: () => void;
  }

  let { workId, onClose }: Props = $props();

  const transcript = $derived(workId ? workerTranscripts.transcriptFor(workId) : null);
  const dispositionLabel = $derived(
    transcript?.disposition === "bound" ? "Workshop" : "Peer",
  );
  const toolRuns = $derived(toolRunsFromWorkerActivity(transcript?.toolRuns ?? []));
  const bodyText = $derived(transcript?.resultText?.trim() || transcript?.output?.trim() || "");
  const thinking = $derived(transcript?.thinking?.trim() ?? "");

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
      class="worker-peek-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Subagent transcript"
      tabindex="-1"
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
      onkeydown={handleKeydown}
    >
      <div class="worker-peek-sheet">
        <header class="worker-peek-chrome">
          <span class="worker-peek-icon" aria-hidden="true">
            <Bot size={15} strokeWidth={1.75} />
          </span>
          <div class="worker-peek-heading">
            <p class="worker-peek-title">{transcript?.title ?? workId}</p>
            <p class="worker-peek-meta">
              {dispositionLabel}{#if transcript?.model} · {transcript.model}{/if}{#if transcript?.statusLine} · {transcript.statusLine}{/if}
            </p>
          </div>
          <button
            type="button"
            class="worker-peek-close"
            aria-label="Close"
            onclick={handleClose}
          >
            <X size={16} strokeWidth={2.25} />
          </button>
        </header>

        <div class="worker-peek-body">
          {#if !transcript}
            <p class="worker-peek-empty">Loading subagent transcript…</p>
          {:else}
            {#if toolRuns.length > 0}
              <section>
                <p class="worker-peek-label">Tools</p>
                <ToolRunChips runs={toolRuns} inspectorCollapsed={false} />
              </section>
            {/if}

            {#if thinking}
              <section>
                <p class="worker-peek-label">Reasoning</p>
                <p class="worker-peek-thinking">{thinking}</p>
              </section>
            {/if}

            {#if bodyText}
              <section>
                <p class="worker-peek-label">Output</p>
                <div class="worker-peek-output markdown-body">
                  {@html renderMarkdown(bodyText)}
                </div>
              </section>
            {/if}

            {#if transcript.error}
              <p class="worker-peek-error">{transcript.error}</p>
            {/if}

            {#if toolRuns.length === 0 && !thinking && !bodyText && !transcript.error}
              <p class="worker-peek-empty">No activity recorded yet.</p>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .worker-peek-backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--color-surface-950) 72%, transparent);
    padding: max(0.5rem, env(safe-area-inset-top, 0px))
      max(0.5rem, env(safe-area-inset-right, 0px))
      max(0.5rem, env(safe-area-inset-bottom, 0px))
      max(0.5rem, env(safe-area-inset-left, 0px));
    animation: worker-peek-backdrop-in 220ms ease-out;
  }

  .worker-peek-sheet {
    display: flex;
    flex-direction: column;
    width: min(36rem, 100%);
    max-height: 100%;
    border-radius: 1.25rem;
    background: rgb(var(--color-surface-50));
    color: rgb(var(--color-surface-900));
    overflow: hidden;
    box-shadow: 0 18px 48px color-mix(in srgb, var(--color-surface-950) 35%, transparent);
    animation: worker-peek-sheet-in 320ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  :global(.dark) .worker-peek-sheet,
  :global([data-mode="dark"]) .worker-peek-sheet {
    background: rgb(var(--color-surface-900));
    color: rgb(var(--color-surface-50));
  }

  .worker-peek-chrome {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    padding: 0.9rem 1rem 0.65rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 18%, transparent);
  }

  .worker-peek-icon {
    display: inline-flex;
    flex-shrink: 0;
    margin-top: 0.1rem;
    color: rgb(var(--color-primary-400));
  }

  .worker-peek-heading {
    min-width: 0;
    flex: 1;
  }

  .worker-peek-title {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.9rem;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .worker-peek-meta {
    margin: 0.1rem 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.7rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
  }

  .worker-peek-close {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-500) 18%, transparent);
    color: inherit;
    cursor: pointer;
  }

  .worker-peek-close:hover {
    background: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
  }

  .worker-peek-body {
    flex: 0 1 auto;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 0.9rem 1rem 1.35rem;
  }

  .worker-peek-label {
    margin: 0 0 0.4rem;
    font-size: 0.6rem;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: color-mix(in srgb, currentColor 45%, transparent);
  }

  .worker-peek-thinking {
    margin: 0;
    white-space: pre-wrap;
    font-size: 0.8rem;
    line-height: 1.5;
    color: color-mix(in srgb, currentColor 62%, transparent);
  }

  .worker-peek-output {
    font-size: 0.88rem;
    line-height: 1.55;
    color: color-mix(in srgb, currentColor 88%, transparent);
  }

  .worker-peek-output :global(p) {
    margin: 0 0 0.75em;
  }

  .worker-peek-output :global(p:last-child) {
    margin-bottom: 0;
  }

  .worker-peek-empty {
    margin: 0;
    font-size: 0.8rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
  }

  .worker-peek-error {
    margin: 0;
    font-size: 0.78rem;
    color: rgb(var(--color-error-400));
  }

  @keyframes worker-peek-backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes worker-peek-sheet-in {
    from {
      opacity: 0;
      transform: translateY(16px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .worker-peek-backdrop,
    .worker-peek-sheet {
      animation: none;
    }
  }
</style>
