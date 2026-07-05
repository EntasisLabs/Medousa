<script lang="ts">
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { toolHistory } from "$lib/stores/toolHistory.svelte";
  import { formatToolName } from "$lib/utils/formatTurn";
  import { humanToolRunHeadline } from "$lib/utils/toolHistorySummary";

  interface Props {
    sessionScoped?: boolean;
    limit?: number;
  }

  let { sessionScoped = true, limit = 10 }: Props = $props();

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

<section class="activity-tool-receipts shrink-0 border-t border-surface-500/45 bg-surface-900/25">
  <header class="flex items-center justify-between gap-2 px-3 py-2.5">
    <div class="min-w-0">
      <h3 class="text-[11px] font-semibold uppercase tracking-wide text-surface-200">
        Tool receipts
      </h3>
      <p class="mt-0.5 text-[10px] leading-snug text-surface-500">
        {#if sessionScoped && sessionId}
          This session — authoritative even when chat lags
        {:else}
          Recent runs — full audit in Automations
        {/if}
      </p>
    </div>
    <button type="button" class="workshop-text-action shrink-0 text-[10px]" onclick={openFullHistory}>
      All history
    </button>
  </header>

  <ol class="max-h-52 space-y-1.5 overflow-y-auto px-3 pb-3">
    {#if toolHistory.loading && runs.length === 0}
      <li class="px-1 py-2 text-[11px] text-surface-500">Loading receipts…</li>
    {:else if toolHistory.error && runs.length === 0}
      <li class="px-1 py-2 text-[11px] text-warning-400">{toolHistory.error}</li>
    {:else if runs.length === 0}
      <li class="px-1 py-2 text-[11px] text-surface-500">
        No tool runs yet for this session.
      </li>
    {:else}
      {#each runs as entry (entry.entry_id)}
        <li class="workshop-inset min-w-0 px-2.5 py-2">
          <div class="flex min-w-0 items-start justify-between gap-2">
            <div class="min-w-0 flex-1">
              <div class="flex min-w-0 flex-wrap items-center gap-1.5">
                <span class="truncate text-[11px] font-medium text-surface-100">
                  {formatToolName(entry.tool_name)}
                </span>
                <span
                  class="text-[9px] uppercase tracking-wide {entry.status === 'succeeded'
                    ? 'text-success-400/90'
                    : entry.status === 'failed'
                      ? 'text-error-400'
                      : 'text-surface-500'}"
                >
                  {toolHistory.statusLabel(entry.status)}
                </span>
              </div>
              <p class="mt-0.5 line-clamp-2 text-[10px] leading-snug text-surface-400">
                {humanToolRunHeadline(entry)}
              </p>
            </div>
            <time class="shrink-0 text-[9px] tabular-nums text-surface-500">
              {formatWhen(entry.timestamp)}
            </time>
          </div>
        </li>
      {/each}
    {/if}
  </ol>
</section>
