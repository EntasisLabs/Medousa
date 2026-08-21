<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import type { CodeEditorStatusSnapshot } from "$lib/stores/codeEditorStatus.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { CircleAlert, CircleCheck, RotateCcw, Wrench } from "@lucide/svelte";
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    triggerEl: HTMLElement | null;
    status: CodeEditorStatusSnapshot;
    onClose: () => void;
    onShowProblems: () => void;
    onShowLogs: () => void;
    onRestart: () => void;
    onRepair: () => void;
  }

  let {
    open,
    triggerEl,
    status,
    onClose,
    onShowProblems,
    onShowLogs,
    onRestart,
    onRepair,
  }: Props = $props();
  let menuEl = $state<HTMLDivElement | null>(null);

  const languageHealthy = $derived(status.languageState === "ready");
  const languageLabel = $derived(
    status.languageState === "connecting"
      ? "Starting"
      : status.languageState === "reconnecting"
        ? "Reconnecting"
        : status.languageState === "failed"
          ? "Failed"
          : status.languageState === "editing-only"
            ? "Editing only"
            : "Ready",
  );

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeToolbarPopover(triggerEl, menuEl, {
        prefer: "above",
        width: 340,
        gap: 8,
        pad: 10,
        maxHeightRatio: 0.6,
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  });

  function act(action: () => void) {
    onClose();
    action();
  }
</script>

<BodyPortal>
  {#snippet children()}
    {#if open}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="code-status-scrim" role="presentation" onclick={onClose}></div>
      <div
        bind:this={menuEl}
        class="code-status-popover workshop-rail-sheet"
        role="dialog"
        aria-label="Code editor status"
        tabindex="-1"
        onkeydown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      >
        <header class="code-status-header">
          <div>
            <h2>Editor status</h2>
            <p>{status.path}</p>
          </div>
          {#if languageHealthy}
            <CircleCheck size={15} strokeWidth={1.9} class="text-content-success" aria-hidden="true" />
          {:else}
            <CircleAlert size={15} strokeWidth={1.9} class="text-content-error" aria-hidden="true" />
          {/if}
        </header>

        <div class="code-status-body">
          <section class="code-status-row">
            <span>Problems</span>
            <button type="button" onclick={() => act(onShowProblems)}>
              {status.issueCount} {status.issueCount === 1 ? "issue" : "issues"}
            </button>
          </section>
          <section class="code-status-language">
            <div class="code-status-row">
              <span>{status.language} language service</span>
              <strong class:text-content-error={status.languageState === "failed"}>{languageLabel}</strong>
            </div>
            {#if status.languageDetail}
              <p>{status.languageDetail}</p>
            {/if}
          </section>
          {#if status.execution}
            <section class="code-status-row">
              <span>Last task</span>
              <strong>{status.execution}</strong>
            </section>
          {/if}
        </div>

        <footer class="code-status-actions">
          <button type="button" onclick={() => act(onShowLogs)}>Show logs</button>
          <button type="button" onclick={() => act(onRestart)}>
            <RotateCcw size={12} strokeWidth={1.9} aria-hidden="true" /> Restart
          </button>
          {#if !languageHealthy}
            <button type="button" class="code-status-repair" onclick={() => act(onRepair)}>
              <Wrench size={12} strokeWidth={1.9} aria-hidden="true" /> Repair support
            </button>
          {/if}
        </footer>
      </div>
    {/if}
  {/snippet}
</BodyPortal>

<style>
  .code-status-scrim {
    position: fixed;
    inset: 0;
    z-index: 72;
  }

  .code-status-popover {
    z-index: 73;
    overflow: hidden;
    padding: 0;
    color: rgb(var(--theme-text-primary));
    font-size: 0.72rem;
  }

  .code-status-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    padding: 0.75rem 0.85rem 0.65rem;
  }

  .code-status-header h2 {
    margin: 0;
    font-size: 0.78rem;
    font-weight: 650;
  }

  .code-status-header p,
  .code-status-language p {
    margin: 0.2rem 0 0;
    overflow-wrap: anywhere;
    color: rgb(var(--theme-text-quiet));
    line-height: 1.45;
  }

  .code-status-header p {
    max-width: 17rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .code-status-body {
    display: grid;
    gap: 0.65rem;
    padding: 0.75rem 0.85rem;
  }

  .code-status-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }

  .code-status-row > span {
    color: rgb(var(--theme-text-quiet));
  }

  .code-status-row strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 550;
  }

  .code-status-row button,
  .code-status-actions button {
    border: 0;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
    font: inherit;
  }

  .code-status-row button:hover,
  .code-status-actions button:hover {
    color: rgb(var(--theme-text-primary));
  }

  .code-status-language {
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.42);
    padding: 0.6rem;
  }

  .code-status-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    padding: 0.55rem 0.65rem;
  }

  .code-status-actions button {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border-radius: 0.35rem;
    padding: 0.28rem 0.45rem;
  }

  .code-status-actions button:hover {
    background: rgb(var(--color-surface-700) / 0.5);
  }

  .code-status-actions .code-status-repair {
    color: rgb(var(--theme-warning));
  }
</style>
