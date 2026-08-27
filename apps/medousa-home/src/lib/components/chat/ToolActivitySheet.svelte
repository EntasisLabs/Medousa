<script lang="ts">
  import { Check, CircleAlert, LoaderCircle, Workflow, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import type { ToolRunState } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import { sliceRefFromChatToolRun } from "$lib/types/toolHistory";
  import { formatToolName } from "$lib/utils/formatTurn";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    runs: ToolRunState[];
    sessionId?: string;
    turnIndex?: number | null;
    onPromoteToFlow?: (ref: ToolHistorySliceRef) => void | Promise<void>;
    onClose: () => void;
  }

  let {
    open,
    runs,
    sessionId,
    turnIndex = null,
    onPromoteToFlow,
    onClose,
  }: Props = $props();
  let sheetEl = $state<HTMLElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  const running = $derived(runs.filter((run) => run.status === "running").length);
  const failed = $derived(runs.filter((run) => run.status === "failed").length);
  const subtitle = $derived(
    running > 0
      ? `${running} running · ${runs.length} total`
      : failed > 0
        ? `${failed} failed · ${runs.length} total`
        : `${runs.length} completed`,
  );

  function close() {
    haptic("light");
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    close();
  }

  function promote(run: ToolRunState) {
    if (!onPromoteToFlow || !sessionId || !turnIndex || run.status === "running") return;
    haptic("light");
    void onPromoteToFlow(
      sliceRefFromChatToolRun({
        sessionId,
        turnIndex,
        runId: run.runId,
        toolRound: run.round,
      }),
    );
  }

  $effect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      close();
      return true;
    });
  });

  $effect(() => {
    if (!open || !sheetEl || !headerEl || !window.matchMedia("(max-width: 767px)").matches) {
      return;
    }
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: close });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet statusIcon(status: ToolRunState["status"])}
  {#if status === "running"}
    <LoaderCircle size={15} strokeWidth={2} class="animate-spin" />
  {:else if status === "failed"}
    <CircleAlert size={15} strokeWidth={2} />
  {:else}
    <Check size={15} strokeWidth={2.25} />
  {/if}
{/snippet}

{#if open}
  <BodyPortal>
    <div
      class="tool-activity-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        bind:this={sheetEl}
        class="tool-activity-sheet"
        role="dialog"
        aria-modal="true"
        aria-label="Tool activity"
      >
        <header bind:this={headerEl} class="tool-activity-header">
          <div class="tool-activity-grabber" aria-hidden="true"></div>
          <div class="tool-activity-heading">
            <div class="min-w-0">
              <h2>Tool activity</h2>
              <p>{subtitle}</p>
            </div>
            <button type="button" aria-label="Close tool activity" onclick={close}>
              <X size={18} strokeWidth={2.1} />
            </button>
          </div>
        </header>

        <div class="tool-activity-body">
          <ol aria-label="Tools in execution order">
            {#each runs as run, index (run.runId)}
              <li class:tool-activity-run-failed={run.status === "failed"}>
                <div class="tool-activity-run-header">
                  <span class="tool-activity-run-index">{index + 1}</span>
                  <div class="min-w-0 flex-1">
                    <h3>{formatToolName(run.toolName)}</h3>
                    <p>{run.toolName}</p>
                  </div>
                  <span class="tool-activity-status" data-status={run.status}>
                    {@render statusIcon(run.status)}
                    {run.status === "running"
                      ? "Running"
                      : run.status === "failed"
                        ? "Failed"
                        : "Done"}
                  </span>
                </div>

                {#if run.inputParams && run.inputParams.length > 0}
                  <section class="tool-activity-detail">
                    <h4>Parameters</h4>
                    <dl>
                      {#each run.inputParams as param (param.key)}
                        <div>
                          <dt>{param.key}</dt>
                          <dd>
                            {param.value}{#if param.truncated}<span
                                class="tool-activity-truncated"
                                title="This value was shortened before display"
                              >…</span
                            >{/if}
                          </dd>
                        </div>
                      {/each}
                    </dl>
                  </section>
                {:else if run.inputSummary?.trim()}
                  <section class="tool-activity-detail">
                    <h4>Request</h4>
                    <p>{run.inputSummary}</p>
                  </section>
                {/if}

                <section class="tool-activity-detail">
                  <h4>Result</h4>
                  {#if run.outputSummary?.trim()}
                    <p>{run.outputSummary}</p>
                  {:else if run.status === "running"}
                    <p class="tool-activity-muted">Waiting for the tool to finish…</p>
                  {:else if run.status === "failed"}
                    <p class="tool-activity-muted">The tool did not return a result summary.</p>
                  {:else}
                    <p class="tool-activity-muted">Completed without a result summary.</p>
                  {/if}
                </section>

                {#if run.artifactRefs && run.artifactRefs.length > 0}
                  <p class="tool-activity-receipts">
                    {run.artifactRefs.length} saved receipt{run.artifactRefs.length === 1 ? "" : "s"}
                  </p>
                {/if}

                {#if onPromoteToFlow && sessionId && turnIndex && run.status !== "running"}
                  <button type="button" class="tool-activity-action" onclick={() => promote(run)}>
                    <Workflow size={14} strokeWidth={2} />
                    Save as flow step
                  </button>
                {/if}
              </li>
            {/each}
          </ol>
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .tool-activity-backdrop {
    position: fixed;
    inset: 0;
    z-index: 140;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgb(var(--color-surface-950) / 0.72);
    backdrop-filter: blur(8px);
  }

  .tool-activity-sheet {
    display: flex;
    width: min(38rem, 100%);
    max-height: min(80dvh, 46rem);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.35);
    border-radius: 1.35rem;
    background: rgb(var(--color-surface-900));
    color: rgb(var(--theme-text-primary));
    box-shadow: 0 24px 70px rgb(0 0 0 / 0.42);
  }

  .tool-activity-header {
    flex: none;
    border-bottom: 1px solid rgb(var(--color-surface-600) / 0.3);
    padding: 0.85rem 1.25rem 0.9rem;
  }

  .tool-activity-grabber {
    display: none;
  }

  .tool-activity-heading {
    display: flex;
    min-height: 2.75rem;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .tool-activity-heading h2 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
    color: rgb(var(--theme-text-primary));
  }

  .tool-activity-heading p {
    margin: 0.15rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--theme-text-quiet));
  }

  .tool-activity-heading button {
    display: inline-flex;
    width: 2.25rem;
    height: 2.25rem;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    color: rgb(var(--theme-text-secondary));
    transition: background 140ms ease, color 140ms ease;
  }

  .tool-activity-heading button:hover {
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--theme-text-primary));
  }

  .tool-activity-body {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 1rem 1.25rem 1.5rem;
  }

  .tool-activity-body ol {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
    gap: 0.75rem;
  }

  .tool-activity-body li {
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.3);
    border-radius: 1rem;
    background: rgb(var(--color-surface-800) / 0.55);
    padding: 1rem;
  }

  .tool-activity-body li.tool-activity-run-failed {
    border-color: rgb(251 113 133 / 0.28);
  }

  .tool-activity-run-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .tool-activity-run-index {
    display: inline-flex;
    width: 1.75rem;
    height: 1.75rem;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 0.6rem;
    background: rgb(var(--color-surface-700) / 0.65);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--theme-text-tertiary));
  }

  .tool-activity-run-header h3 {
    overflow: hidden;
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
    color: rgb(var(--theme-text-primary));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-activity-run-header p {
    overflow: hidden;
    margin: 0.1rem 0 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.65rem;
    color: rgb(var(--theme-text-quiet));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-activity-status {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.68rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .tool-activity-status[data-status="running"] {
    color: rgb(var(--color-primary-300));
  }

  .tool-activity-status[data-status="failed"] {
    color: rgb(251 113 133);
  }

  .tool-activity-detail {
    margin-top: 0.9rem;
    border-top: 1px solid rgb(var(--color-surface-600) / 0.22);
    padding-top: 0.75rem;
  }

  .tool-activity-detail h4 {
    margin: 0 0 0.5rem;
    font-size: 0.67rem;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-quiet));
  }

  .tool-activity-detail > p {
    margin: 0;
    overflow-wrap: anywhere;
    font-size: 0.79rem;
    line-height: 1.55;
    color: rgb(var(--theme-text-secondary));
    white-space: pre-wrap;
  }

  .tool-activity-detail dl {
    display: grid;
    margin: 0;
    gap: 0.4rem;
  }

  .tool-activity-detail dl > div {
    display: grid;
    grid-template-columns: minmax(5.5rem, 0.34fr) minmax(0, 1fr);
    align-items: baseline;
    gap: 0.75rem;
    border-radius: 0.6rem;
    background: rgb(var(--color-surface-950) / 0.32);
    padding: 0.5rem 0.65rem;
  }

  .tool-activity-detail dt {
    overflow: hidden;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.68rem;
    color: rgb(var(--color-primary-300) / 0.78);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-activity-detail dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    font-size: 0.76rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-secondary));
    white-space: pre-wrap;
  }

  .tool-activity-truncated,
  .tool-activity-muted,
  .tool-activity-receipts {
    color: rgb(var(--theme-text-quiet));
  }

  .tool-activity-action {
    display: inline-flex;
    min-height: 2.25rem;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.75rem;
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-700) / 0.48);
    padding: 0.45rem 0.7rem;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-secondary));
    transition: background 140ms ease, color 140ms ease;
  }

  .tool-activity-action:hover {
    background: rgb(var(--color-surface-700) / 0.75);
    color: rgb(var(--theme-text-primary));
  }

  .tool-activity-receipts {
    margin: 0.75rem 0 0;
    font-size: 0.68rem;
  }

  @media (max-width: 767px) {
    .tool-activity-backdrop {
      align-items: flex-end;
      padding: 0;
    }

    .tool-activity-sheet {
      width: 100%;
      max-height: min(84dvh, 48rem);
      border-right: 0;
      border-bottom: 0;
      border-left: 0;
      border-radius: 1.5rem 1.5rem 0 0;
      padding-bottom: env(safe-area-inset-bottom, 0px);
    }

    .tool-activity-header {
      padding: 0 1.25rem 0.8rem;
    }

    .tool-activity-grabber {
      display: block;
      width: 2.5rem;
      height: 0.28rem;
      margin: 0.55rem auto 0.5rem;
      border-radius: 999px;
      background: rgb(var(--color-surface-500) / 0.65);
    }

    .tool-activity-heading button {
      width: 2.75rem;
      height: 2.75rem;
    }

    .tool-activity-body {
      padding: 1rem 1.25rem 1.5rem;
    }

    .tool-activity-body li {
      padding: 1rem;
    }

    .tool-activity-detail dl > div {
      grid-template-columns: 1fr;
      gap: 0.25rem;
    }
  }
</style>
