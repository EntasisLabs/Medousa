<script lang="ts">
  import { SlidersHorizontal } from "@lucide/svelte";
  import { flows } from "$lib/stores/flows.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { toolHistory } from "$lib/stores/toolHistory.svelte";
  import { sliceRefFromRun, type ToolHistoryRunEntry } from "$lib/types/toolHistory";
  import {
    humanToolRunAsk,
    humanToolRunHeadline,
    humanToolRunResult,
    humanToolRunSubline,
    sessionChapterTitle,
    suggestFlowNameFromRun,
    suggestFlowNameFromRuns,
  } from "$lib/utils/toolHistorySummary";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    onOpenFlows?: () => void;
  }

  interface Chapter {
    key: string;
    title: string;
    dayLabel: string;
    entries: ToolHistoryRunEntry[];
  }

  let { visible, mobile = false, embedded = false, onOpenFlows }: Props = $props();

  let search = $state("");
  let filterOpen = $state(false);
  let flowName = $state("");
  let expandedId = $state<string | null>(null);
  let searchOpen = $state(false);
  const filterActive = $derived(search.trim().length > 0);

  function closeMenus() {
    filterOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  const filtered = $derived.by(() => {
    const query = search.trim().toLowerCase();
    const rows = [...toolHistory.runs];
    if (!query) return rows;
    return rows.filter((entry) => {
      const haystack = [
        entry.tool_name,
        entry.input_summary,
        entry.session_id,
        entry.session_preview ?? "",
        humanToolRunHeadline(entry),
        entry.output_preview ?? "",
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  });

  const chapters = $derived.by((): Chapter[] => {
    const groups: Chapter[] = [];
    const indexByKey = new Map<string, number>();
    for (const entry of filtered) {
      const title = sessionChapterTitle(entry);
      const key = `${dayKey(entry.timestamp)}::${entry.session_id || title}`;
      const existing = indexByKey.get(key);
      if (existing == null) {
        indexByKey.set(key, groups.length);
        groups.push({
          key,
          title,
          dayLabel: dayLabel(entry.timestamp),
          entries: [entry],
        });
      } else {
        groups[existing].entries.push(entry);
      }
    }
    return groups;
  });

  const selectedCount = $derived(toolHistory.selectedIds.size);
  const hasRuns = $derived(toolHistory.runs.length > 0);
  const chapterCount = $derived(chapters.length);

  $effect(() => {
    if (!visible) return;
    void toolHistory.refresh({ limit: 120 });
  });

  $effect(() => {
    if (selectedCount === 0) {
      flowName = "";
      return;
    }
    if (flowName.trim()) return;
    flowName = suggestFlowNameFromRuns(toolHistory.selectedRuns());
  });

  function dayKey(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
  }

  function dayLabel(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    const today = new Date();
    const yesterday = new Date();
    yesterday.setDate(today.getDate() - 1);
    if (sameDay(date, today)) return "Today";
    if (sameDay(date, yesterday)) return "Yesterday";
    return date.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
    });
  }

  function sameDay(left: Date, right: Date): boolean {
    return (
      left.getFullYear() === right.getFullYear() &&
      left.getMonth() === right.getMonth() &&
      left.getDate() === right.getDate()
    );
  }

  function timeLabel(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
  }

  function toggleExpanded(id: string) {
    expandedId = expandedId === id ? null : id;
  }

  function toggleSelect(entry: ToolHistoryRunEntry) {
    toolHistory.toggleSelected(entry.entry_id);
  }

  async function buildFlow(run = false) {
    const response = run
      ? await toolHistory.promoteSelection(flowName, true)
      : await toolHistory.promoteSelection(flowName, false);
    flows.composerDraft = {
      name: response.draft.name ?? flowName,
      goal: "",
      steps: response.draft.steps,
      cron_expr: "",
      timezone: "UTC",
    };
    flows.lastPlan = null;
    flows.composerOpen = true;
    flowDraft.clear();
    flowName = "";
    onOpenFlows?.();
  }

  async function automateEntry(entry: ToolHistoryRunEntry, event: MouseEvent) {
    event.stopPropagation();
    const name = suggestFlowNameFromRun(entry);
    const response = await toolHistory.promoteRef(sliceRefFromRun(entry), name);
    flows.composerDraft = {
      name: response.draft.name ?? name,
      goal: settings.showWorkshopGuidance
        ? `Repeat: ${humanToolRunHeadline(entry)}`
        : "",
      steps: response.draft.steps,
      cron_expr: "",
      timezone: "UTC",
    };
    flows.lastPlan = null;
    flows.composerOpen = true;
    onOpenFlows?.();
  }
</script>

<svelte:window onclick={closeMenus} />

<section
  class="automations-history flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden {visible
    ? ''
    : 'hidden'}"
>
  {#if !embedded}
    <header class="workshop-header">
      <div class="flex flex-wrap items-end justify-between gap-3">
        <div class="min-w-0 max-w-xl">
          <p class="history-kicker">From conversation</p>
          <h3 class="history-title">
            {#if hasRuns}
              Moments she already lived
            {:else}
              Waiting on the first tools
            {/if}
          </h3>
          <p class="history-lead">
            {#if hasRuns}
              {chapterCount} conversation{chapterCount === 1 ? "" : "s"} · tap beats to gather a flow.
            {:else}
              Successful chat tools gather here as a story you can schedule.
            {/if}
          </p>
        </div>
        {#if hasRuns}
          <button
            type="button"
            class="workshop-text-action shrink-0 text-xs"
            onclick={() => (searchOpen = !searchOpen)}
          >
            {searchOpen || search.trim() ? "Hide search" : "Search"}
          </button>
        {/if}
      </div>

      {#if hasRuns && (searchOpen || search.trim())}
        <label class="mt-4 block max-w-md">
          <span class="sr-only">Search history</span>
          <input
            class="input w-full text-sm"
            type="search"
            placeholder="Search moments…"
            bind:value={search}
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
          />
        </label>
      {/if}
    </header>
  {/if}

  <div
    class="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto {embedded
      ? 'px-2 py-1'
      : 'mobile-you-scroll px-4 py-5'}"
  >
    {#if toolHistory.loading && toolHistory.runs.length === 0}
      <p class="workshop-muted">Gathering recent moments…</p>
    {:else if toolHistory.error}
      <p class="text-sm text-warning-400">{toolHistory.error}</p>
    {:else if !hasRuns}
      <div class="history-empty">
        <p class="history-title">Nothing to retell yet</p>
        <p class="history-lead mt-2">
          When tools succeed in chat, they land here as beats. Gather a few and turn that stretch
          into a flow.
        </p>
      </div>
    {:else if filtered.length === 0}
      <p class="workshop-muted py-6 text-sm">No moments match that search.</p>
    {:else}
      {#each chapters as chapter (chapter.key)}
        <article class="history-chapter">
          <header class="history-chapter-header">
            <div class="min-w-0 flex-1">
              <h4 class="history-chapter-title">{chapter.title}</h4>
              <p class="history-chapter-meta">
                {chapter.dayLabel}
                · {chapter.entries.length} moment{chapter.entries.length === 1 ? "" : "s"}
              </p>
            </div>
          </header>

          <ol class="history-timeline">
            {#each chapter.entries as entry, index (entry.entry_id)}
              {@const selected = toolHistory.selectedIds.has(entry.entry_id)}
              {@const expanded = expandedId === entry.entry_id}
              {@const ask = humanToolRunAsk(entry)}
              {@const result = humanToolRunResult(entry)}
              <li
                class="history-beat {selected ? 'history-beat-selected' : ''} {index ===
                chapter.entries.length - 1
                  ? 'history-beat-last'
                  : ''}"
              >
                <button
                  type="button"
                  class="history-beat-hit"
                  onclick={() => toggleSelect(entry)}
                  aria-pressed={selected}
                >
                  <span class="history-beat-time" aria-hidden="true">
                    {timeLabel(entry.timestamp)}
                  </span>
                  <span class="history-beat-rail" aria-hidden="true">
                    <span class="history-beat-dot {selected ? 'history-beat-dot-on' : ''}"></span>
                  </span>
                  <span class="history-beat-body">
                    <span class="history-beat-title">{humanToolRunHeadline(entry)}</span>
                    <span class="history-beat-sub">
                      {humanToolRunSubline(entry)}
                      {#if entry.status === "failed"}
                        <span class="history-beat-failed"> · failed</span>
                      {:else if entry.redacted}
                        <span class="history-beat-failed"> · redacted</span>
                      {/if}
                    </span>
                  </span>
                </button>

                <div class="history-beat-actions">
                  <button
                    type="button"
                    class="history-beat-more"
                    onclick={() => toggleExpanded(entry.entry_id)}
                    aria-expanded={expanded}
                  >
                    {expanded ? "Less" : "More"}
                  </button>
                  {#if entry.status === "succeeded"}
                    <button
                      type="button"
                      class="history-beat-more"
                      disabled={toolHistory.promoting}
                      onclick={(event) => void automateEntry(entry, event)}
                    >
                      Flow
                    </button>
                  {/if}
                </div>

                {#if expanded}
                  <div class="history-beat-expand">
                    {#if ask}
                      <p class="history-beat-expand-label">She took in</p>
                      <p class="history-beat-expand-copy">{ask}</p>
                    {/if}
                    {#if result}
                      <p class="history-beat-expand-label {ask ? 'mt-3' : ''}">Came back</p>
                      <p class="history-beat-expand-copy">{result}</p>
                    {/if}
                    {#if !ask && !result}
                      <p class="history-beat-expand-copy workshop-faint">
                        No prose detail on this beat.
                      </p>
                    {/if}
                  </div>
                {/if}
              </li>
            {/each}
          </ol>
        </article>
      {/each}
    {/if}

    {#if toolHistory.actionMessage}
      <p class="mt-4 text-xs text-primary-300">{toolHistory.actionMessage}</p>
    {/if}
  </div>

  {#if embedded}
    <footer
      class="relative flex shrink-0 items-center gap-1 border-t border-surface-500/25 px-2 py-1.5"
    >
      <div class="min-w-0 flex-1">
        {#if filterActive}
          <span class="workshop-faint truncate text-[11px]">Filtered</span>
        {/if}
      </div>
      <div class="relative shrink-0">
        <button
          type="button"
          class="vault-dock-icon-btn {filterActive ? 'vault-dock-icon-btn-active' : ''}"
          aria-haspopup="menu"
          aria-expanded={filterOpen}
          aria-label="Filter history"
          title="Filter"
          onclick={(event) => {
            event.stopPropagation();
            filterOpen = !filterOpen;
          }}
        >
          <SlidersHorizontal size={15} strokeWidth={1.75} />
        </button>
        {#if filterOpen}
          <div
            class="vault-notes-filter-menu absolute bottom-full right-0 z-30 mb-1 w-[min(17.5rem,calc(100vw-2rem))] rounded-lg border border-surface-500/50 bg-surface-900 py-2 shadow-xl"
            role="menu"
            tabindex="-1"
            onclick={(event) => event.stopPropagation()}
            onkeydown={handleMenuKeydown}
          >
            <div class="px-2.5 pb-1">
              <input
                class="input w-full text-xs"
                type="search"
                placeholder="Search history…"
                bind:value={search}
                autocapitalize="off"
                autocorrect="off"
                spellcheck="false"
                onclick={(event) => event.stopPropagation()}
              />
            </div>
            {#if filterActive}
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item text-surface-400"
                onclick={() => {
                  search = "";
                }}
              >
                Clear
              </button>
            {/if}
          </div>
        {/if}
      </div>
    </footer>
  {/if}

  {#if selectedCount > 0}
    <div class="history-dock" role="region" aria-label="Save selection as flow">
      <span class="history-dock-count">{selectedCount}</span>
      <input
        class="history-dock-input"
        bind:value={flowName}
        placeholder="Name this flow…"
        spellcheck="false"
        aria-label="Flow name"
      />
      <div class="history-dock-actions">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(false)}
        >
          {toolHistory.promoting ? "…" : "Open"}
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(true)}
        >
          Run
        </button>
        <button
          type="button"
          class="history-dock-clear"
          aria-label="Clear selection"
          onclick={() => {
            toolHistory.clearSelection();
            flowName = "";
          }}
        >
          ✕
        </button>
      </div>
    </div>
  {/if}
</section>

<style>
  .history-kicker {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-title {
    margin: 0.35rem 0 0;
    font-size: 1.125rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .history-lead {
    margin: 0.4rem 0 0;
    max-width: 34rem;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .history-empty {
    max-width: 28rem;
    padding: 2.5rem 0;
  }

  .history-dock {
    position: relative;
    z-index: 5;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.55rem 0.75rem;
    margin: 0;
    padding: 0.55rem 0.85rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.35);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.92);
    backdrop-filter: blur(10px);
  }

  .history-dock-count {
    display: inline-flex;
    min-width: 1.4rem;
    height: 1.4rem;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: rgb(var(--color-primary-500) / 0.16);
    color: rgb(var(--color-primary-300));
    font-size: 0.6875rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .history-dock-input {
    min-width: 0;
    flex: 1 1 10rem;
    border: none;
    background: transparent;
    padding: 0.2rem 0;
    font-size: 0.8125rem;
    font-weight: 500;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .history-dock-input:focus {
    outline: none;
  }

  .history-dock-input::placeholder {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    font-weight: 400;
  }

  .history-dock-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .history-dock-clear {
    margin: 0;
    margin-left: 0.15rem;
    padding: 0.15rem 0.4rem;
    border: none;
    border-radius: 0.4rem;
    background: transparent;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
    font-size: 0.75rem;
    line-height: 1;
    cursor: pointer;
  }

  .history-dock-clear:hover {
    color: rgb(var(--shell-label, var(--color-surface-100)));
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.6);
  }

  .history-chapter {
    margin-bottom: 2rem;
  }

  .history-chapter-header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: baseline;
    gap: 1rem;
    padding: 0.35rem 0 0.75rem;
    background: rgb(var(--shell-canvas-bg, var(--color-surface-900)) / 0.92);
    backdrop-filter: blur(8px);
  }

  .history-chapter-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .history-chapter-meta {
    margin: 0.2rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-timeline {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .history-beat {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.35rem 0.75rem;
    padding: 0.15rem 0 0.85rem;
  }

  .history-beat-hit {
    display: grid;
    grid-template-columns: 3.25rem 1rem minmax(0, 1fr);
    gap: 0.65rem;
    align-items: start;
    width: 100%;
    margin: 0;
    padding: 0.35rem 0.4rem 0.35rem 0;
    border: none;
    border-radius: 0.65rem;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .history-beat-hit:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.45);
  }

  .history-beat-selected .history-beat-hit {
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .history-beat-time {
    padding-top: 0.15rem;
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    text-align: right;
  }

  .history-beat-rail {
    position: relative;
    display: flex;
    justify-content: center;
    padding-top: 0.45rem;
    align-self: stretch;
  }

  .history-beat-rail::before {
    content: "";
    position: absolute;
    top: 0.9rem;
    bottom: -1.1rem;
    width: 1px;
    background: rgb(var(--shell-border, var(--color-surface-500)) / 0.4);
  }

  .history-beat-last .history-beat-rail::before {
    display: none;
  }

  .history-beat-dot {
    position: relative;
    z-index: 1;
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 999px;
    background: rgb(var(--shell-border, var(--color-surface-500)) / 0.8);
    box-shadow: 0 0 0 3px rgb(var(--shell-canvas-bg, var(--color-surface-900)) / 0.95);
  }

  .history-beat-dot-on {
    background: rgb(var(--color-primary-400));
    box-shadow:
      0 0 0 3px rgb(var(--shell-canvas-bg, var(--color-surface-900)) / 0.95),
      0 0 0 5px rgb(var(--color-primary-500) / 0.2);
  }

  .history-beat-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding-top: 0.05rem;
  }

  .history-beat-title {
    font-size: 0.875rem;
    font-weight: 500;
    line-height: 1.35;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .history-beat-sub {
    font-size: 0.71875rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-beat-failed {
    color: rgb(var(--color-warning-400));
  }

  .history-beat-actions {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
    padding-top: 0.35rem;
    opacity: 0.55;
    transition: opacity 120ms ease;
  }

  .history-beat:hover .history-beat-actions,
  .history-beat-selected .history-beat-actions {
    opacity: 1;
  }

  .history-beat-more {
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    font-size: 0.6875rem;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-primary-300));
    cursor: pointer;
  }

  .history-beat-more:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .history-beat-expand {
    grid-column: 1 / -1;
    margin: 0.15rem 0 0.25rem 4.9rem;
    padding: 0.75rem 0.9rem;
    border-radius: 0.65rem;
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.55);
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.3);
  }

  .history-beat-expand-label {
    margin: 0;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-beat-expand-copy {
    margin: 0.3rem 0 0;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--shell-label, var(--color-surface-200)));
  }

  @media (max-width: 640px) {
    .history-beat-hit {
      grid-template-columns: 2.6rem 0.85rem minmax(0, 1fr);
      gap: 0.45rem;
    }

    .history-beat-expand {
      margin-left: 0;
    }
  }
</style>
