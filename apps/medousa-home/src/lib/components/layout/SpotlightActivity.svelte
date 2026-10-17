<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { Activity, ChevronRight, ChevronDown, RefreshCw, ArrowUpRight, Maximize2, ArrowLeft } from "@lucide/svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { workerTranscripts } from "$lib/work/workerTranscripts.svelte";
  import { ActivityData } from "$lib/spotlight/activityData.svelte";
  import { activityDataPort, readActivityCodeRun } from "$lib/spotlight/activityApi";
  import { activityTaskIds, activityPath, buildWorkshopActivity, filterActivityTree, flattenActivityTree, type ActivityFilter, type ActivityNode } from "$lib/spotlight/activity";
  import { getJobResult, getWorkspaceCard } from "$lib/daemon";
  import type { WorkshopCommandContext } from "$lib/commands/types";
  import { formatSessionWhen } from "$lib/utils/formatSession";
  import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import type { ChatSessionRuntime } from "$lib/chat/chatSessionRuntime";
  import TaskActivityActions from "$lib/components/tasks/TaskActivityActions.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { openNewAsk } from "$lib/utils/workChromeEvents";
  import { openTaskDetails } from "$lib/runtime/shellUseCases";

  let { query = "", ctx }: { query?: string; ctx: WorkshopCommandContext } = $props();
  const data = new ActivityData(activityDataPort);
  let filter = $state<ActivityFilter>(commandSpotlight.activityFilter ?? "all");
  $effect(() => { if (commandSpotlight.activityFilter) filter = commandSpotlight.activityFilter; });
  let expanded = $state<Set<string>>(new Set());
  let selectedId = $state<string | null>(null);
  let detailExpanded = $state(false);
  let previewText = $state("");
  let previewMeta = $state("");
  let previewLoading = $state(false);
  let previewError = $state("");
  let openError = $state("");
  let opening = $state(false);
  let previewRevision = $state(0);
  let rowsEl = $state<HTMLDivElement | null>(null);
  let alive = true;
  const activityCards = $derived(filter !== "all" && filter !== "running" || query.trim() ? workspace.cards : workspace.visibleCards());

  const details = $derived.by(() => {
    const next = new Map(data.details);
    for (const [id, detail] of workspace.cardDetailsCache) {
      if (!next.has(id) || detail.card.updated_at_utc >= next.get(id)!.card.updated_at_utc) next.set(id, detail);
    }
    for (const [id, detail] of next) {
      const live = detail.work_id ? workerTranscripts.transcripts.get(detail.work_id) : null;
      if (live) next.set(id, { ...detail, live_status_line: live.statusLine, live_tool_activity: live.toolRuns.slice(-40), result_excerpt: live.resultText || detail.result_excerpt });
    }
    return next;
  });
  const liveTurns = $derived.by(() => {
    void chat.runtimeRevision;
    const runtimes = new Map<string, Pick<ChatSessionRuntime, "turns" | "messages">>(chat.sessionRuntimes);
    if (chat.sessionId) runtimes.set(chat.sessionId, { turns: chat.turns, messages: chat.messages });
    return [...runtimes].flatMap(([sessionId, runtime]) => [...runtime.turns.values()].filter((turn) => !turn.terminal).map((turn) => {
      const message = runtime.messages.find((entry) => entry.turnId === turn.turnId);
      const index = message ? runtime.messages.indexOf(message) : -1;
      const prompt = index >= 0 ? runtime.messages.slice(0, index + 1).reverse().find((entry) => entry.role === "user")?.content : "";
      return { id: turn.turnId, sessionId, phase: turn.phase, terminal: turn.terminal, title: prompt?.slice(0, 100) || "Chat turn", updatedAt: message?.createdAt || "" };
    }));
  });
  const tree = $derived(buildWorkshopActivity({
    cards: activityCards, details, sessions: chat.sessions, feed: workspace.feed,
    projects: data.projects, projectRuns: data.projectRuns,
    schedules: data.schedules, scheduleRuns: data.scheduleRuns,
    turns: liveTurns,
  }));
  const filtered = $derived(filterActivityTree(tree, filter, query));
  const matchingTaskIds = $derived(activityTaskIds(filtered));
  const matchingCards = $derived(activityCards.filter((card) => matchingTaskIds.has(card.id)));
  const rows = $derived(flattenActivityTree(filtered, expanded, Boolean(query.trim())));
  const selected = $derived(rows.find(({ node }) => node.id === selectedId)?.node ?? null);
  const previewKey = $derived(JSON.stringify([selected?.id, selected?.target, selected?.kind === "Tool run"]));
  const sourceErrors = $derived(Object.entries(data.errors).filter(([, message]) => Boolean(message)));
  const filters: Array<{ id: ActivityFilter; label: string }> = [
    { id: "all", label: "All" }, { id: "running", label: "Running" },
    { id: "attention", label: "Needs you" }, { id: "finished", label: "Finished" },
  ];

  $effect(() => {
    const cards = activityCards;
    const cached = workspace.cardDetailsCache;
    void data.cardLimit;
    if (!connection.offline) untrack(() => { void data.loadCards(cards, cached); });
  });
  $effect(() => {
    if (rows.some(({ node }) => node.id === selectedId)) return;
    const path = selectedId ? activityPath(filtered, selectedId) : null;
    if (path) expanded = new Set([...expanded, ...path.slice(0, -1).map((node) => node.id)]);
    else selectedId = rows[0]?.node.id ?? null;
  });
  $effect(() => {
    const id = selectedId;
    void id;
    detailExpanded = false;
    openError = "";
  });

  onMount(() => {
    const refresh = () => {
      if (!connection.offline && document.visibilityState !== "hidden") {
        void data.refresh();
        void data.loadCards(activityCards, workspace.cardDetailsCache);
        previewRevision += 1;
      }
    };
    refresh();
    const timer = window.setInterval(refresh, 15000);
    document.addEventListener("visibilitychange", refresh);
    return () => { alive = false; window.clearInterval(timer); document.removeEventListener("visibilitychange", refresh); data.dispose(); };
  });

  // Fetch only the highlighted execution. Ignore late replies after selection/close.
  $effect(() => {
    // Streaming summaries rebuild the tree; only a different target restarts output.
    void previewKey;
    const node = untrack(() => selected);
    void previewRevision;
    previewText = "";
    previewMeta = "";
    previewError = "";
    previewLoading = false;
    if (!node || connection.offline || node.kind === "Tool run") return;
    let cancelled = false;
    const target = node.target;
    const timer = window.setTimeout(() => {
      if (!target) return;
      const load = async () => {
        previewLoading = true;
        try {
          if (target.kind === "code-run") {
            const run = await readActivityCodeRun(target.workId, target.runId);
            if (cancelled) return;
            previewText = [run.stdout, run.stderr].filter(Boolean).join("\n").slice(-12000);
            previewMeta = run.output_truncated ? "Showing the retained output tail" : "Execution output";
          } else if (target.kind === "job" || (target.kind === "card" && target.jobId)) {
            const result = await getJobResult(target.kind === "job" ? target.jobId : target.jobId!);
            if (cancelled) return;
            previewText = (result.output_text || result.interim_text || result.latest_outcome || "").slice(-12000);
            previewMeta = `${result.attempt_count} attempt${result.attempt_count === 1 ? "" : "s"} · ${result.status}`;
          } else if (target.kind === "card") {
            const detail = await getWorkspaceCard(target.cardId);
            if (cancelled) return;
            previewText = (detail.live_output || detail.result_excerpt || detail.task_line || "").slice(-12000);
            previewMeta = detail.model || "Task output";
          }
        } catch (cause) { if (!cancelled) previewError = formatDaemonErrorSummary(cause); }
        finally { if (!cancelled) previewLoading = false; }
      };
      void load();
    }, 180);
    return () => { cancelled = true; window.clearTimeout(timer); };
  });

  function choose(node: ActivityNode) { selectedId = node.id; }
  function canExpand(node: ActivityNode) { return node.children.length > 0 || node.kind === "Schedule" || node.kind === "Code project"; }
  function toggle(node: ActivityNode) {
    choose(node);
    const next = new Set(expanded);
    if (next.has(node.id)) next.delete(node.id); else next.add(node.id);
    expanded = next;
    if (node.target?.kind === "schedule" && !data.scheduleRuns.has(node.target.recurringId)) void data.loadScheduleRuns(node.target.recurringId);
    if (node.kind === "Code project" && node.target?.kind === "project" && !data.projectRuns.has(node.target.workId)) void data.loadProjectRuns(node.target.workId);
  }
  async function open(node: ActivityNode) {
    if (opening) return;
    const target = node.target;
    const workshopId = activeWorkshopId();
    const current = () => alive && activeWorkshopId() === workshopId;
    openError = "";
    opening = true;
    try {
      if (target?.kind === "chat" || (target?.kind === "card" && target.sessionId)) {
        const id = target.kind === "chat" ? target.sessionId : target.sessionId!;
        await ctx.chat.switchSession(id);
        if (!current()) return;
        ctx.navigate("chat"); ctx.callbacks.close();
      } else if (target?.kind === "card" || target?.kind === "job") {
        await openTaskDetails(target.kind === "card" ? target.cardId : target.jobId);
      } else if (target?.kind === "project") {
        const { setCoderExecutionTransport } = await import("$lib/executionAuthority");
        if (!current()) return;
        setCoderExecutionTransport(null);
        const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
        if (!current()) return;
        if (target.review) await lmeWorkspace.openCodeReview(target.workId, node.title);
        else await lmeWorkspace.openCodeWorkspace(target.workId, node.title);
        if (current()) ctx.callbacks.close();
      } else if (target?.kind === "note") {
        const { lmeWorkspace } = await import("$lib/stores/lmeWorkspace.svelte");
        if (!current()) return;
        await lmeWorkspace.openNote(target.path); if (current()) ctx.callbacks.close();
      } else {
        detailExpanded = true;
        if (node.children.length > 0 && !expanded.has(node.id)) toggle(node);
      }
    } catch (cause) { if (current()) openError = formatDaemonErrorSummary(cause); }
    finally { if (current()) opening = false; }
  }
  function handleKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || rows.length === 0) return;
    if (event.target instanceof Element && event.target.closest("select, textarea")) return;
    if (event.key === "Enter" && event.target instanceof Element && event.target.closest("button")) return;
    const index = rows.findIndex(({ node }) => node.id === selectedId);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      selectedId = rows[(Math.max(index, 0) + (event.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length].node.id;
      requestAnimationFrame(() => rowsEl?.querySelector<HTMLElement>("[aria-selected='true']")?.scrollIntoView({ block: "nearest" }));
    } else if (event.key === "Enter" && selected) { event.preventDefault(); void open(selected); }
    else if (event.key === "ArrowRight" && selected && canExpand(selected) && !expanded.has(selected.id)) { event.preventDefault(); toggle(selected); }
    else if (event.key === "ArrowLeft" && selected) {
      event.preventDefault();
      if (expanded.has(selected.id)) toggle(selected);
      else if (index > 0) {
        const parent = rows.slice(0, index).reverse().find((row) => row.depth < rows[index].depth);
        if (parent) selectedId = parent.node.id;
      }
    }
  }
  function refresh() { void workspace.reconcileCardsFromSnapshot(); void data.refresh(); void data.loadCards(activityCards, workspace.cardDetailsCache); previewRevision += 1; }
</script>

<svelte:window onkeydown={handleKeydown} />

<section class="spotlight-activity" aria-label="Workshop activity">
  <header class="activity-toolbar">
    <div class="activity-filters" aria-label="Activity filters">
      {#each filters as item}
        <button class:active={filter === item.id} aria-pressed={filter === item.id} onclick={() => { commandSpotlight.activityFilter = null; filter = item.id; }}>{item.label}</button>
      {/each}
    </div>
    <div class="flex items-center gap-2"><button class="activity-more" onclick={openNewAsk}>New ask</button>
    <button class="activity-icon" aria-label="Refresh activity" disabled={data.loading || connection.offline} onclick={refresh}><RefreshCw size={14} class={data.loading ? 'animate-spin' : ''} /></button></div>
  </header>
  <TaskActivityActions {filter} cards={matchingCards} />
  {#if connection.offline}<p class="activity-notice">Offline · showing last known activity</p>{/if}
  {#if sourceErrors.length > 0}<p class="activity-notice">{sourceErrors.map(([name]) => name).join(", ")} activity could not be refreshed. <button onclick={refresh}>Retry</button></p>{/if}
  <div class="activity-columns" class:expanded-detail={detailExpanded}>
    <div class="activity-list" bind:this={rowsEl}>
      <div role="tree" aria-label="Activity hierarchy">
        {#each rows as { node, depth } (node.id)}
          <div class="activity-row" class:selected={node.id === selectedId} role="treeitem" aria-label={`${node.title}, ${node.kind}, ${node.status}`} aria-level={depth + 1} aria-expanded={canExpand(node) ? (Boolean(query.trim()) || expanded.has(node.id)) : undefined} aria-selected={node.id === selectedId} tabindex={node.id === selectedId ? 0 : -1} style:padding-left={`${8 + depth * 16}px`} onclick={() => choose(node)} ondblclick={() => void open(node)} onkeydown={handleKeydown}>
            {#if canExpand(node)}
              <button class="activity-icon activity-chevron" aria-label={`${expanded.has(node.id) ? 'Collapse' : 'Expand'} ${node.title}`} onclick={(event) => { event.stopPropagation(); toggle(node); }}>
                {#if expanded.has(node.id) || query.trim()}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
              </button>
            {:else}<span class="activity-leaf"><Activity size={12} /></span>{/if}
            <span class="activity-copy"><span class="activity-title">{node.title}</span><span class="activity-subtitle">{node.kind} · <span data-state={node.state}>{node.status}</span>{#if node.children.length} · {node.children.length}{/if}</span></span>
            <span class="activity-time">{formatSessionWhen(node.updatedAt)}</span>
          </div>
        {:else}
          <p class="activity-empty">{data.loading ? "Loading workshop activity…" : query.trim() ? "No matching activity" : filter === 'all' ? "No recent activity in this workshop" : `No ${filter === 'attention' ? 'activity needs your attention' : filter + ' activity'}`}</p>
        {/each}
      </div>
      {#if data.detailsLoading}<p class="activity-loading">Loading task connections…</p>{/if}
      {#if activityCards.length > data.cardLimit}<button class="activity-more" onclick={() => { data.cardLimit += 24; }}>Load more task connections</button>{/if}
      {#if data.nextCursor}<button class="activity-more" disabled={data.loading} onclick={() => void data.refresh(true)}>Load more code projects</button>{/if}
    </div>
    <aside class="activity-preview" aria-label="Activity preview">
      {#if selected}
        <header>
          {#if detailExpanded}<button class="activity-back" onclick={() => { detailExpanded = false; }}><ArrowLeft size={13} /> Back to activity</button>{/if}
          <p class="activity-kicker">{selected.kind}</p><h2>{selected.title}</h2>
          <p class="activity-status" data-state={selected.state}>{selected.status}</p>
          {#if selected.updatedAt}<p class="activity-subtitle">Updated {new Date(selected.updatedAt).toLocaleString()}</p>{/if}
        </header>
        <div class="activity-preview-scroll">
          {#if selected.error}<p class="activity-error">{selected.error}</p>{/if}
          {#if selected.summary}<p class="activity-summary">{selected.summary}</p>{/if}
          {#if previewLoading}<p class="activity-loading">Loading execution output…</p>{/if}
          {#if previewError}<p class="activity-error">{previewError}</p><button class="activity-more" onclick={() => { previewRevision += 1; }}>Retry output</button>{/if}
          {#if previewText}<p class="activity-kicker">{previewMeta}</p><pre>{previewText}</pre>{/if}
          {#if selected.children.length}
            <p class="activity-kicker">Related activity</p>
            {#each selected.children as child}<button class="activity-child" onclick={() => { if (!expanded.has(selected!.id)) toggle(selected!); selectedId = child.id; detailExpanded = false; }}><span>{child.title}</span><span data-state={child.state}>{child.status}</span></button>{/each}
          {/if}
          {#if !selected.summary && !previewText && !previewLoading && !previewError && !selected.children.length}<p class="activity-loading">Select Open to continue in the related context.</p>{/if}
        </div>
        <footer>
          {#if openError}<p class="activity-error">{openError}</p>{/if}
          <button class="activity-open" disabled={opening} onclick={() => void open(selected!)}>{#if selected.target?.kind === 'project' || selected.target?.kind === 'chat' || selected.target?.kind === 'note' || selected.target?.kind === 'card' || selected.target?.kind === 'job'}<ArrowUpRight size={14} /> Open {selected.target.kind === 'project' ? (selected.target.review ? 'review' : 'project') : selected.target.kind === 'note' ? 'note' : selected.target.kind === 'job' || (selected.target.kind === 'card' && !selected.target.sessionId) ? 'task' : 'chat'}{:else}<Maximize2 size={14} /> Expand details{/if}</button>
          {#if selected.target?.kind === 'card' && selected.target.sessionId}
            <button class="activity-more" onclick={() => { const target = selected?.target; if (target?.kind === 'card') void openTaskDetails(target.cardId); }}>Task details</button>
          {/if}
        </footer>
      {:else}<p class="activity-empty">Select an activity to preview its progress and results.</p>{/if}
    </aside>
  </div>
</section>

<style>
  .spotlight-activity { display:flex; flex:1; flex-direction:column; min-height:0; overflow:hidden; }
  .activity-toolbar { display:flex; flex-shrink:0; align-items:center; justify-content:space-between; padding:8px 12px; border-bottom:1px solid rgb(var(--theme-border) / .25); }
  .activity-filters { display:flex; gap:3px; flex-wrap:wrap; }
  .activity-filters button { padding:5px 8px; font-size:11px; border-radius:6px; color:rgb(var(--theme-text-tertiary)); }
  .activity-filters button.active { color:rgb(var(--theme-text)); background:rgb(var(--theme-border) / .25); }
  .activity-columns { display:grid; flex:1; grid-template-columns:minmax(0,1.1fr) minmax(0,1fr); min-height:0; }
  .activity-list { overflow:auto; min-width:0; padding:6px; }
  .activity-row { display:flex; gap:6px; align-items:center; padding:8px 6px; border-radius:6px; cursor:pointer; min-width:0; }
  .activity-row:hover { background:rgb(var(--theme-border) / .12); }
  .activity-row.selected { background:rgb(var(--theme-border) / .28); }
  .activity-icon,.activity-leaf { display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; width:22px; height:24px; color:rgb(var(--theme-text-tertiary)); }
  .activity-copy { display:flex; flex-direction:column; min-width:0; flex:1; gap:3px; }
  .activity-title { font-size:12px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .activity-subtitle,.activity-time { font-size:10px; color:rgb(var(--theme-text-tertiary)); }
  .activity-time { flex-shrink:0; }
  [data-state="attention"],.activity-error { color:rgb(var(--color-warning-400)); }
  [data-state="running"] { color:rgb(var(--color-primary-400)); }
  [data-state="finished"] { color:rgb(var(--theme-text-secondary)); }
  .activity-preview { min-width:0; min-height:0; display:flex; flex-direction:column; border-left:1px solid rgb(var(--theme-border) / .25); }
  .activity-preview header { flex-shrink:0; padding:14px 16px; border-bottom:1px solid rgb(var(--theme-border) / .2); }
  .activity-kicker { font-size:10px; color:rgb(var(--theme-text-tertiary)); margin:0 0 8px; }
  h2 { font-size:14px; line-height:1.5; overflow-wrap:anywhere; margin:0 0 8px; }
  .activity-status { font-size:11px; margin-bottom:5px; }
  .activity-preview-scroll { flex:1; min-height:0; overflow:auto; padding:14px 16px; }
  .activity-summary,.activity-error { font-size:12px; line-height:1.65; white-space:pre-wrap; overflow-wrap:anywhere; margin-bottom:12px; }
  pre { font-size:11px; line-height:1.65; white-space:pre-wrap; overflow-wrap:anywhere; margin-bottom:16px; }
  .activity-child { display:flex; flex-direction:column; text-align:left; gap:4px; font-size:11px; padding:8px 0; width:100%; border-bottom:1px solid rgb(var(--theme-border) / .16); }
  .activity-child span:first-child { overflow-wrap:anywhere; }
  .activity-preview footer { flex-shrink:0; padding:10px 16px; border-top:1px solid rgb(var(--theme-border) / .25); }
  .activity-open,.activity-back { display:flex; align-items:center; gap:7px; font-size:12px; }
  .activity-back { font-size:11px; margin-bottom:14px; }
  .activity-empty { padding:24px 16px; color:rgb(var(--theme-text-tertiary)); font-size:12px; line-height:1.6; }
  .activity-loading,.activity-notice { flex-shrink:0; color:rgb(var(--theme-text-tertiary)); font-size:11px; padding:8px 12px; }
  .activity-more { font-size:11px; padding:8px 12px; color:rgb(var(--theme-text-secondary)); }
  .expanded-detail { grid-template-columns:1fr; }
  .expanded-detail .activity-list { display:none; }
  @media(max-width:640px) { .activity-columns { grid-template-columns:1fr; overflow:auto; } .activity-list { max-height:230px; min-height:110px; } .activity-preview { border-left:0; border-top:1px solid rgb(var(--theme-border) / .25); min-height:180px; } .activity-preview-scroll { min-height:120px; } }
</style>
