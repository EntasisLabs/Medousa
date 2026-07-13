<script lang="ts">
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { toolHistory } from "$lib/stores/toolHistory.svelte";
  import { humanToolRunHeadline } from "$lib/utils/toolHistorySummary";

  interface Props {
    sessionScoped?: boolean;
    limit?: number;
  }

  let { sessionScoped = true, limit = 3 }: Props = $props();

  const sessionId = $derived(sessionScoped ? chat.sessionId.trim() : "");

  const runs = $derived.by(() => {
    const rows = toolHistory.runs;
    if (!sessionScoped || !sessionId) return rows.slice(0, limit);
    return rows.filter((entry) => entry.session_id === sessionId).slice(0, limit);
  });

  $effect(() => {
    void toolHistory.refresh({
      limit: Math.max(limit, 24),
      sessionId: sessionScoped && sessionId ? sessionId : undefined,
    });
  });

  function openFullHistory() {
    automationsNav.openSection("history");
    layout.navigateDesktop("automations", { bump: true });
  }

  function formatWhen(value: string | null | undefined): string {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    const diffMs = Date.now() - date.getTime();
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return "Just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
</script>

{#if runs.length > 0 || (toolHistory.loading && sessionScoped)}
  <section class="activity-receipts shrink-0">
    <header class="activity-receipts-header">
      <p class="activity-receipts-kicker">This session</p>
      <button
        type="button"
        class="workshop-text-action activity-receipts-action shrink-0 text-[10px]"
        onclick={openFullHistory}
      >
        History
      </button>
    </header>

    <ol class="activity-receipts-list">
      {#if toolHistory.loading && runs.length === 0}
        <li class="activity-receipts-empty">Loading…</li>
      {:else if toolHistory.error && runs.length === 0}
        <li class="activity-receipts-empty text-warning-400">{toolHistory.error}</li>
      {:else}
        {#each runs as entry (entry.entry_id)}
          <li class="activity-receipts-row">
            <p class="activity-receipts-summary">
              {humanToolRunHeadline(entry)}
              <span class="activity-receipts-when">
                {#if entry.status === "failed"}Failed · {/if}{formatWhen(entry.timestamp)}
              </span>
            </p>
          </li>
        {/each}
      {/if}
    </ol>
  </section>
{/if}

<style>
  .activity-receipts {
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    background: transparent;
  }

  .activity-receipts-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.65rem 1rem 0.2rem;
  }

  .activity-receipts-kicker {
    margin: 0;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.9);
  }

  .activity-receipts-action {
    opacity: 0.5;
    transition: opacity 140ms ease;
  }

  .activity-receipts:hover .activity-receipts-action,
  .activity-receipts-action:focus-visible {
    opacity: 1;
  }

  .activity-receipts-list {
    margin: 0;
    max-height: 7.5rem;
    overflow-y: auto;
    padding: 0 0.65rem 0.7rem;
    list-style: none;
    scrollbar-width: thin;
    scrollbar-color: rgb(var(--shell-border, var(--color-surface-500)) / 0.4) transparent;
  }

  .activity-receipts-empty {
    padding: 0.35rem 0.35rem;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .activity-receipts-row {
    padding: 0.35rem 0.35rem;
    border-radius: 0.4rem;
  }

  .activity-receipts-row:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.25);
  }

  .activity-receipts-summary {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 500;
    line-height: 1.4;
    color: rgb(var(--shell-label, var(--color-surface-200)));
    display: -webkit-box;
    -webkit-line-clamp: 1;
    line-clamp: 1;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .activity-receipts-when {
    margin-left: 0.35rem;
    font-weight: 400;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }
</style>
