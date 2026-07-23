<script lang="ts">
  import { Search, X } from "@lucide/svelte";
  import { tick } from "svelte";
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
  import { portLmeDock } from "$lib/utils/lmeDockHost";

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
  let flowName = $state("");
  let expandedId = $state<string | null>(null);
  let searchOpen = $state(false);
  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  const filterActive = $derived(search.trim().length > 0);

  $effect(() => {
    if (filterActive && !searchExpanded) searchExpanded = true;
  });

  async function openDockSearch() {
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeDockSearch() {
    searchExpanded = false;
    search = "";
  }

  function handleDockSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeDockSearch();
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
      <p class="history-empty-line">
        {embedded
          ? "Nothing here yet — tool runs from chat show up as moments."
          : "Nothing to retell yet. When tools succeed in chat, they land here."}
      </p>
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
                    class="history-beat-act"
                    onclick={() => toggleExpanded(entry.entry_id)}
                    aria-expanded={expanded}
                  >
                    {expanded ? "Less" : "More"}
                  </button>
                  {#if entry.status === "succeeded"}
                    <button
                      type="button"
                      class="history-beat-act"
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
                      <p class="history-beat-expand-label">Asked</p>
                      <p class="history-beat-expand-copy">{ask}</p>
                    {/if}
                    {#if result}
                      <p class="history-beat-expand-label {ask ? 'mt-2.5' : ''}">Result</p>
                      <p class="history-beat-expand-copy">{result}</p>
                    {/if}
                    {#if !ask && !result}
                      <p class="history-beat-expand-copy workshop-faint">No detail on this beat.</p>
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
      class="lme-side-rail-dock"
      use:portLmeDock
    >
      {#if searchExpanded}
        <div class="lme-dock-search-expand min-w-0 flex-1">
          <Search size={14} strokeWidth={1.75} class="lme-dock-search-glyph" />
          <input
            bind:this={searchInputEl}
            class="lme-dock-search-input"
            type="search"
            placeholder="Search history…"
            bind:value={search}
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            onkeydown={handleDockSearchKeydown}
          />
        </div>
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Close search"
          title="Close search"
          onclick={closeDockSearch}
        >
          <X size={15} strokeWidth={1.75} />
        </button>
      {:else}
        <div class="min-w-0 flex-1"></div>
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Search history"
          title="Search"
          onclick={() => void openDockSearch()}
        >
          <Search size={15} strokeWidth={1.75} />
        </button>
      {/if}
    </footer>
  {/if}

  {#if selectedCount > 0}
    <div class="history-dock" role="region" aria-label="Save selection as flow">
      <span class="history-dock-count">{selectedCount} selected</span>
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
          class="history-dock-act history-dock-act-primary"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(false)}
        >
          {toolHistory.promoting ? "…" : "Open"}
        </button>
        <button
          type="button"
          class="history-dock-act"
          disabled={toolHistory.promoting}
          onclick={() => void buildFlow(true)}
        >
          Run
        </button>
        <button
          type="button"
          class="history-dock-act"
          aria-label="Clear selection"
          onclick={() => {
            toolHistory.clearSelection();
            flowName = "";
          }}
        >
          Clear
        </button>
      </div>
    </div>
  {/if}
</section>

<style>
  .history-kicker {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 500;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-title {
    margin: 0.35rem 0 0;
    font-size: 1.125rem;
    font-weight: 560;
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

  .history-empty-line {
    margin: 0;
    padding: 1.75rem 0.25rem;
    font-size: 0.8rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-dock {
    position: relative;
    z-index: 5;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem 0.85rem;
    margin: 0;
    padding: 0.5rem 0.75rem;
    border-top: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.22);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.88);
  }

  .history-dock-count {
    flex-shrink: 0;
    font-size: 0.68rem;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    font-variant-numeric: tabular-nums;
  }

  .history-dock-input {
    min-width: 0;
    flex: 1 1 8rem;
    border: none;
    background: transparent;
    padding: 0.15rem 0;
    font-size: 0.8rem;
    font-weight: 450;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-100)));
    outline: none;
    box-shadow: none;
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
    gap: 0.75rem;
  }

  .history-dock-act {
    margin: 0;
    padding: 0.1rem 0;
    border: none;
    background: transparent;
    font-size: 0.72rem;
    font-weight: 450;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    cursor: pointer;
    transition: color 140ms ease;
  }

  .history-dock-act:hover:not(:disabled) {
    color: rgb(var(--shell-label, var(--color-surface-200)));
  }

  .history-dock-act-primary {
    color: rgb(var(--shell-label, var(--color-surface-250, var(--color-surface-200))));
    font-weight: 520;
  }

  .history-dock-act-primary:hover:not(:disabled) {
    color: rgb(var(--shell-label, var(--color-surface-50)));
  }

  .history-dock-act:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .history-chapter {
    margin-bottom: 1.65rem;
  }

  .history-chapter-header {
    display: flex;
    align-items: baseline;
    gap: 1rem;
    padding: 0.2rem 0 0.55rem;
  }

  .history-chapter-title {
    margin: 0;
    font-size: 0.82rem;
    font-weight: 520;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-200)));
  }

  .history-chapter-meta {
    margin: 0.15rem 0 0;
    font-size: 0.68rem;
    letter-spacing: -0.01em;
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
    gap: 0.25rem 0.55rem;
    padding: 0.1rem 0 0.7rem;
  }

  .history-beat-hit {
    display: grid;
    grid-template-columns: 3.25rem 1rem minmax(0, 1fr);
    gap: 0.65rem;
    align-items: start;
    width: 100%;
    margin: 0;
    padding: 0.25rem 0.25rem 0.25rem 0;
    border: none;
    border-radius: 0.4rem;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .history-beat-hit:hover {
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.28);
  }

  .history-beat-selected .history-beat-hit {
    background: transparent;
  }

  .history-beat-selected .history-beat-title {
    color: rgb(var(--shell-label, var(--color-surface-50)));
    font-weight: 560;
  }

  .history-beat-time {
    padding-top: 0.15rem;
    font-size: 0.6875rem;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.01em;
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
    bottom: -0.95rem;
    width: 1px;
    background: rgb(var(--shell-border, var(--color-surface-500)) / 0.28);
  }

  .history-beat-last .history-beat-rail::before {
    display: none;
  }

  .history-beat-dot {
    position: relative;
    z-index: 1;
    width: 0.4rem;
    height: 0.4rem;
    border-radius: 999px;
    background: rgb(var(--shell-border, var(--color-surface-500)) / 0.65);
    box-shadow: 0 0 0 3px rgb(var(--shell-canvas-bg, var(--color-surface-900)) / 0.95);
  }

  .history-beat-dot-on {
    background: rgb(var(--shell-label, var(--color-surface-200)));
    box-shadow: 0 0 0 3px rgb(var(--shell-canvas-bg, var(--color-surface-900)) / 0.95);
  }

  .history-beat-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    padding-top: 0.05rem;
  }

  .history-beat-title {
    font-size: 0.84rem;
    font-weight: 450;
    line-height: 1.35;
    letter-spacing: -0.015em;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .history-beat-sub {
    font-size: 0.7rem;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .history-beat-failed {
    color: rgb(var(--color-warning-400) / 0.9);
  }

  .history-beat-actions {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    padding-top: 0.3rem;
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .history-beat:hover .history-beat-actions,
  .history-beat:focus-within .history-beat-actions {
    opacity: 1;
  }

  @media (hover: none) {
    .history-beat-actions {
      opacity: 0.75;
    }
  }

  .history-beat-act {
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    font-size: 0.68rem;
    font-weight: 450;
    letter-spacing: -0.01em;
    text-transform: none;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    cursor: pointer;
    transition: color 140ms ease;
  }

  .history-beat-act:hover:not(:disabled) {
    color: rgb(var(--shell-label, var(--color-surface-250, var(--color-surface-200))));
  }

  .history-beat-act:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .history-beat-expand {
    grid-column: 1 / -1;
    margin: 0.05rem 0 0.15rem 4.9rem;
    padding: 0.15rem 0 0.2rem;
    border: none;
    border-radius: 0;
    background: transparent;
  }

  .history-beat-expand-label {
    margin: 0;
    font-size: 0.65rem;
    font-weight: 450;
    letter-spacing: -0.01em;
    text-transform: none;
    color: rgb(var(--shell-muted, var(--color-surface-500)) / 0.9);
  }

  .history-beat-expand-copy {
    margin: 0.2rem 0 0;
    font-size: 0.78rem;
    line-height: 1.45;
    letter-spacing: -0.01em;
    color: rgb(var(--shell-label, var(--color-surface-300)));
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
