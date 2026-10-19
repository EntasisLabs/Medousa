<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { ArrowUpRight, CalendarClock, ChevronDown, FileCode2, GitBranch, Play, RefreshCw, Search, Zap } from "@lucide/svelte";
  import { automations } from "$lib/stores/automations.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { automationDraft } from "$lib/stores/automationDraft.svelte";
  import { getGraphemeScript, getJobResult, getWorkflow, listGraphemeScripts, listWorkflowRuns, listWorkflows, listRecurringRuns, runGraphemeSource, runWorkflow } from "$lib/daemon";
  import type { GraphemeScriptEntry } from "$lib/types/grapheme";
  import type { RecurringRunEntry } from "$lib/types/recurring";
  import type { WorkflowListEntry } from "$lib/types/workflow";
  import { automationRows, describeAutomationSchedule, filterAutomationRows, workflowReplayRequest, type AutomationFilter, type AutomationRow } from "$lib/utils/automationRunList";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
  import ScheduleCreatePopover from "./ScheduleCreatePopover.svelte";

  let scripts = $state<GraphemeScriptEntry[]>([]);
  let savedFlows = $state<WorkflowListEntry[]>([]);
  let loading = $state(false);
  let errors = $state<string[]>([]);
  let query = $state("");
  let filter = $state<AutomationFilter>("all");
  let selectedId = $state<string | null>(null);
  let inspector = $state<"overview" | "runs">("overview");
  let newMenu = $state(false);
  let busy = $state(false);
  let actionMessage = $state("");
  let actionError = $state("");
  let runsRevision = $state(0);
  let runs = $state<RecurringRunEntry[]>([]);
  let runsLoading = $state(false);
  let runsError = $state("");
  let output = $state("");
  let outputLoading = $state(false);
  let outputVersion = 0;
  let alive = true;
  const scope = activeWorkshopId();
  const current = () => alive && activeWorkshopId() === scope;
  const rows = $derived(automationRows(automations.definitions, scripts, savedFlows));
  const filtered = $derived(filterAutomationRows(rows, filter, query));
  const selected = $derived(filtered.find((row) => row.id === selectedId) ?? null);
  const filters: { id: AutomationFilter; label: string }[] = [{ id: "all", label: "All" }, { id: "scheduled", label: "Scheduled" }, { id: "manual", label: "Manual" }, { id: "paused", label: "Paused" }];
  const selectedKey = $derived(selected?.id ?? null);

  $effect(() => {
    if (!filtered.some((row) => row.id === selectedId)) selectedId = filtered[0]?.id ?? null;
  });
  $effect(() => {
    void selectedKey;
    inspector = "overview";
    output = "";
    outputLoading = false;
    actionError = "";
    actionMessage = "";
    outputVersion += 1;
  });
  $effect(() => {
    const key = selectedKey;
    void runsRevision;
    runs = [];
    runsError = "";
    const row = untrack(() => rows.find((entry) => entry.id === key));
    if (!row || (!row.schedule && !row.flow)) { runsLoading = false; return; }
    let cancelled = false;
    runsLoading = true;
    const request = row.schedule ? listRecurringRuns(row.schedule.recurring_id, 20) : listWorkflowRuns(row.flow!.workflow_id, 20);
    void request.then((response) => { if (!cancelled && current()) runs = response.runs.map((run) => ({ ...run, output_text: null, latest_outcome: run.latest_outcome?.slice(0, 1000) })); }).catch((cause) => { if (!cancelled && current()) runsError = formatDaemonErrorSummary(cause); }).finally(() => { if (!cancelled && current()) runsLoading = false; });
    return () => { cancelled = true; };
  });

  async function refresh() {
    if (loading) return;
    loading = true;
    const failures: string[] = [];
    await Promise.allSettled([
      automations.refresh().then(() => { if (automations.error) failures.push(`Schedules: ${automations.error}`); }),
      listGraphemeScripts({ limit: 100 }).then((response) => { if (current()) scripts = response.scripts.map((script) => ({ ...script, body_preview: null })); }).catch((cause) => { failures.push(`Scripts: ${formatDaemonErrorSummary(cause)}`); }),
      listWorkflows(100).then((response) => { if (current()) savedFlows = response.workflows; }).catch((cause) => { failures.push(`Flows: ${formatDaemonErrorSummary(cause)}`); }),
    ]);
    if (current()) { errors = failures; loading = false; runsRevision += 1; }
  }
  onMount(() => { void refresh(); return () => { alive = false; outputVersion += 1; }; });

  function when(row: AutomationRow) {
    return row.schedule ? describeAutomationSchedule(row.schedule.cron_expr, row.schedule.timezone) : row.flow?.scheduled_recurring_id ? "Schedule unavailable" : "On demand";
  }
  function date(value: string | null) { return value ? automations.formatTimestamp(value) : ""; }
  function openResource(mode: "scripts" | "flows") {
    newMenu = false;
    lmeWorkspace.setExplorerMode(mode);
    layout.openShellSidebarView("automations");
  }
  function edit(row: AutomationRow) {
    if (!current()) return;
    if (row.script) void lmeWorkspace.openScriptById(row.script.id).catch((cause) => { if (current()) actionError = formatDaemonErrorSummary(cause); });
    else if (row.flow) lmeWorkspace.openFlow(row.flow.workflow_id, row.title);
    else if (row.schedule) lmeWorkspace.openSchedule(row.schedule.recurring_id, row.title);
    layout.openShellSidebarView("automations");
  }
  async function run(row: AutomationRow) {
    if (busy || !current()) return;
    busy = true; actionError = ""; actionMessage = "";
    try {
      if (row.script) {
        const detail = await getGraphemeScript(row.script.id);
        if (!current()) return;
        if (detail.body_truncated) throw new Error("Open this script in the editor to run its full source.");
        const response = await runGraphemeSource(detail.body_preview);
        if (current()) actionMessage = response.result.job_id ? "Script enqueued. Open Activity to follow its progress." : response.result.succeeded === false ? "Script failed. Open the editor for details." : response.result.succeeded === true ? "Script finished." : "Script submitted. Open the editor for details.";
      } else if (row.flow) {
        const detail = await getWorkflow(row.flow.workflow_id);
        if (!current()) return;
        await runWorkflow(workflowReplayRequest(detail));
        if (current()) { actionMessage = "Flow enqueued. Open Activity to follow its progress."; void refresh(); }
      }
    } catch (cause) { if (current()) actionError = formatDaemonErrorSummary(cause); }
    finally { if (current()) busy = false; }
  }
  async function pause(row: AutomationRow) {
    if (!row.schedule || busy || !current()) return;
    busy = true; actionError = "";
    try { await automations.setEnabled(row.schedule.recurring_id, !row.schedule.enabled); }
    catch (cause) { if (current()) actionError = formatDaemonErrorSummary(cause); }
    finally { if (current()) busy = false; }
  }
  async function showOutput(run: RecurringRunEntry) {
    const version = ++outputVersion;
    outputLoading = true; output = ""; actionError = "";
    try {
      const result = await getJobResult(run.job_id);
      if (current() && version === outputVersion) output = (result.output_text || result.interim_text || result.latest_outcome || "No output retained for this run.").slice(-16000);
    } catch (cause) { if (current() && version === outputVersion) actionError = formatDaemonErrorSummary(cause); }
    finally { if (current() && version === outputVersion) outputLoading = false; }
  }
</script>

<svelte:window onpointerdown={(event) => { if (event.target instanceof Element && !event.target.closest(".new-action")) newMenu = false; }} onkeydown={(event) => { if (event.key === "Escape") newMenu = false; }} />
<section class="run-list" aria-label="Automations">
  <header class="page-head">
    <div><h1>Automations</h1><p>Saved work, ready to run again.</p></div>
    <div class="head-actions">
      <button class="quiet" onclick={() => commandSpotlight.openActivity()}>Activity <ArrowUpRight size={14} /></button>
      <div class="new-action">
        <button class="primary" aria-expanded={newMenu} onclick={() => { newMenu = !newMenu; }}>New automation <ChevronDown size={14} /></button>
        {#if newMenu}<div class="new-menu">
          <button onclick={() => { newMenu = false; automationDraft.openCreate(); }}>Quick prompt <span>Describe work and choose when</span></button>
          <button onclick={() => openResource("scripts")}>From script <span>Choose or create a script</span></button>
          <button onclick={() => openResource("flows")}>From flow <span>Choose or create a flow</span></button>
        </div>{/if}
        <ScheduleCreatePopover trigger="hidden" lmeHosted={true} />
      </div>
    </div>
  </header>
  <div class="list-toolbar">
    <div class="filters" aria-label="Automation filters">{#each filters as item}<button class:active={filter === item.id} aria-pressed={filter === item.id} onclick={() => { filter = item.id; }}>{item.label}</button>{/each}</div>
    <label class="search"><Search size={14} /><input aria-label="Search automations" placeholder="Search automations…" bind:value={query} /></label>
    <button class="icon" aria-label="Refresh automations" disabled={loading} onclick={refresh}><RefreshCw size={15} class={loading ? 'animate-spin' : ''} /></button>
  </div>
  {#each errors as error}<p class="notice">{error} <button onclick={refresh}>Retry</button></p>{/each}
  <div class="body" class:has-selection={selected !== null}>
    <div class="catalog">
      <div class="column-head"><span>Automation</span><span>When</span><span>Last result</span></div>
      <div class="rows">
        {#each filtered as row (row.id)}
          <button class="automation-row" class:selected={selectedId === row.id} aria-pressed={selectedId === row.id} onclick={() => { selectedId = row.id; }}>
            <span class="identity"><span class="type-icon">{#if row.kind === 'Script'}<FileCode2 size={17} />{:else if row.kind === 'Flow'}<GitBranch size={17} />{:else}<Zap size={17} />{/if}</span><span class="row-copy"><strong>{row.title}</strong><small>{row.kind}{#if row.schedule?.enabled === false} · Paused{/if}</small></span></span>
            <span class="when"><span>{when(row)}</span>{#if row.schedule?.enabled}<small>Next {date(row.schedule.next_run_at_utc)}</small>{/if}</span>
            <span class="last"><span>{automations.statusLabel(row.lastStatus)}</span><small>{date(row.lastAt)}</small></span>
          </button>
        {:else}<div class="empty"><CalendarClock size={24} /><h2>{loading ? 'Loading saved work…' : query || filter !== 'all' ? 'No matching automations' : 'Make repeat work easier'}</h2><p>{query || filter !== 'all' ? 'Try another filter or search.' : 'Start with a prompt, script, or flow. Choose a schedule when you need one.'}</p></div>{/each}
      </div>
      <footer class="resource-links"><span>Building blocks</span><button onclick={() => openResource("scripts")}><FileCode2 size={14} /> Scripts</button><button onclick={() => openResource("flows")}><GitBranch size={14} /> Flows</button></footer>
    </div>
    {#if selected}
      <aside aria-label="Automation details">
        <header class="detail-head"><small>{selected.kind}{#if selected.schedule} · {selected.schedule.enabled ? 'Scheduled' : 'Paused'}{:else} · On demand{/if}</small><h2>{selected.title}</h2>
          <div class="detail-actions">{#if selected.script || selected.flow}<button class="primary" disabled={busy} onclick={() => void run(selected!)}><Play size={13} /> {busy ? 'Working…' : 'Run now'}</button>{/if}
          {#if selected.schedule}<button class="secondary" disabled={busy} onclick={() => void pause(selected!)}>{selected.schedule.enabled ? 'Pause' : 'Resume'}</button>{/if}<button class="quiet" onclick={() => edit(selected!)}>Open editor <ArrowUpRight size={13} /></button></div>
        </header>
        <nav class="detail-tabs" aria-label="Automation details tabs"><button class:active={inspector === 'overview'} onclick={() => { inspector = 'overview'; }}>Overview</button><button class:active={inspector === 'runs'} onclick={() => { inspector = 'runs'; }}>Runs {runs.length || ''}</button></nav>
        <div class="detail-scroll">
          {#if actionError}<p class="notice" role="alert">{actionError}</p>{/if}{#if actionMessage}<p class="notice" role="status">{actionMessage}</p>{/if}
          {#if inspector === 'overview'}
            {#if selected.schedule?.prompt_excerpt}<div class="detail-section"><h3>What runs</h3><p class="description">{selected.schedule.prompt_excerpt}</p></div>{:else if selected.script?.intent}<div class="detail-section"><h3>What runs</h3><p>{selected.script.intent}</p></div>{:else if selected.flow}<div class="detail-section"><h3>What runs</h3><p>{selected.flow.step_count} steps · {selected.flow.strategy}</p></div>{/if}
            <div class="detail-section"><h3>When</h3><p>{when(selected)}</p>{#if selected.schedule}<small>{selected.schedule.timezone}</small>{#if selected.schedule.enabled}<p class="muted">Next {date(selected.schedule.next_run_at_utc)}</p>{/if}{/if}</div>
            {#if selected.schedule}<div class="detail-section"><h3>Results</h3><p>{automations.deliveryLabelFor(selected.schedule)}</p></div>{/if}
            <div class="detail-section"><h3>Last result</h3><p>{selected.script ? 'Run results are available in Activity and the script editor.' : automations.statusLabel(selected.lastStatus)}</p>{#if selected.lastAt}<small>{date(selected.lastAt)}</small>{/if}</div>
            {#if selected.schedule}<button class="source-link" onclick={() => { lmeWorkspace.openSchedule(selected!.schedule!.recurring_id, selected!.title); }}>Edit schedule and delivery <ArrowUpRight size={13} /></button>{/if}
          {:else}
            {#if runsLoading}<p class="muted">Loading runs…</p>{:else if runsError}<p class="notice">{runsError}</p>{:else if !runs.length}<p class="muted">{selected.script ? 'Script activity is available in Activity. Open the editor for the current execution result.' : 'No recorded runs yet.'}</p>{/if}
            {#each runs as run (run.job_id)}<button class="run" onclick={() => void showOutput(run)}><span>{run.status}</span><small>{date(run.scheduled_at_utc)}</small></button>{/each}
            {#if outputLoading}<p class="muted">Loading output…</p>{/if}{#if output}<pre>{output}</pre>{/if}
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</section>

<style>
  .run-list { display:flex; flex-direction:column; height:100%; min-width:0; color:rgb(var(--theme-text)); }
  .page-head { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:28px 28px 22px; }
  h1 { font-size:22px; font-weight:600; letter-spacing:-.025em; margin:0 0 6px; }
  .page-head p,.muted { color:rgb(var(--theme-text-tertiary)); font-size:12px; }
  .head-actions,.detail-actions,.filters,.list-toolbar,.search,.resource-links { display:flex; align-items:center; gap:8px; }
  button { cursor:pointer; } button:disabled { opacity:.5; cursor:default; } button:focus-visible,input:focus-visible { outline:2px solid rgb(var(--theme-text-secondary)); outline-offset:2px; }
  .primary,.secondary,.quiet { display:inline-flex; align-items:center; justify-content:center; gap:7px; padding:8px 11px; border-radius:7px; font-size:12px; }
  .primary { background:rgb(var(--theme-text)); color:rgb(var(--theme-pane)); } .secondary { border:1px solid rgb(var(--theme-border) / .5); }
  .quiet { color:rgb(var(--theme-text-secondary)); } .quiet:hover,.secondary:hover { background:rgb(var(--theme-border) / .18); }
  .new-action { position:relative; } .new-menu { position:absolute; top:calc(100% + 6px); right:0; width:260px; padding:5px; z-index:40; border:1px solid rgb(var(--theme-border) / .5); border-radius:10px; background:rgb(var(--theme-pane)); box-shadow:0 12px 40px #0005; }
  .new-menu button { display:block; width:100%; text-align:left; padding:10px; font-size:12px; border-radius:6px; } .new-menu button:hover { background:rgb(var(--theme-border) / .2); } .new-menu span { display:block; font-size:11px; margin-top:3px; color:rgb(var(--theme-text-tertiary)); }
  .list-toolbar { padding:0 28px 18px; flex-wrap:wrap; border-bottom:1px solid rgb(var(--theme-border) / .25); }
  .filters { gap:3px; } .filters button,.detail-tabs button { padding:7px 10px; font-size:12px; color:rgb(var(--theme-text-tertiary)); border-radius:6px; } .active { background:rgb(var(--theme-border) / .25); color:rgb(var(--theme-text)) !important; }
  .search { margin-left:auto; color:rgb(var(--theme-text-tertiary)); padding:7px 10px; border:1px solid rgb(var(--theme-border) / .35); border-radius:7px; } input { width:180px; background:transparent; border:0; padding:0; box-shadow:none; border-radius:0; font-size:12px; color:rgb(var(--theme-text)); outline:none; }
  .icon { display:flex; padding:7px; color:rgb(var(--theme-text-secondary)); } .body { display:grid; grid-template-columns:1fr; min-height:0; flex:1; } .body.has-selection { grid-template-columns:minmax(0,1fr) minmax(280px,32%); }
  .catalog { display:flex; flex-direction:column; min-width:0; min-height:0; } .column-head,.automation-row { display:grid; grid-template-columns:minmax(160px,1.5fr) minmax(110px,1fr) minmax(90px,.75fr); gap:18px; align-items:center; }
  .column-head { padding:13px 28px; font-size:10px; color:rgb(var(--theme-text-tertiary)); border-bottom:1px solid rgb(var(--theme-border) / .2); } .rows { flex:1; overflow:auto; min-height:0; padding:5px 12px; }
  .automation-row { width:100%; padding:17px 16px; border-bottom:1px solid rgb(var(--theme-border) / .16); border-radius:7px; text-align:left; font-size:12px; } .automation-row:hover { background:rgb(var(--theme-border) / .12); } .automation-row.selected { background:rgb(var(--theme-border) / .23); }
  .identity { display:flex; gap:12px; align-items:center; min-width:0; } .type-icon { display:flex; padding:9px; color:rgb(var(--theme-text-secondary)); border:1px solid rgb(var(--theme-border) / .3); border-radius:9px; flex-shrink:0; }
  .row-copy,.when,.last { display:flex; flex-direction:column; gap:5px; min-width:0; } strong { font-weight:500; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; } small { font-size:10px; line-height:1.6; color:rgb(var(--theme-text-tertiary)); } .when { font-size:11px; } .last { font-size:11px; }
  .resource-links { padding:14px 28px; border-top:1px solid rgb(var(--theme-border) / .25); font-size:11px; flex-wrap:wrap; } .resource-links span { color:rgb(var(--theme-text-tertiary)); margin-right:auto; } .resource-links button { display:flex; gap:6px; align-items:center; padding:4px; }
  aside { display:flex; flex-direction:column; min-height:0; min-width:0; border-left:1px solid rgb(var(--theme-border) / .3); } .detail-head { padding:24px 22px 18px; } h2 { font-size:18px; line-height:1.4; font-weight:500; margin:8px 0 18px; overflow-wrap:anywhere; } .detail-actions { flex-wrap:wrap; } .detail-actions button { font-size:11px; }
  .detail-tabs { padding:0 18px 9px; border-bottom:1px solid rgb(var(--theme-border) / .25); display:flex; gap:4px; } .detail-scroll { padding:4px 22px 24px; overflow:auto; flex:1; min-height:0; } .detail-section { padding:20px 0; border-bottom:1px solid rgb(var(--theme-border) / .2); } h3 { font-size:10px; font-weight:500; color:rgb(var(--theme-text-tertiary)); margin:0 0 9px; } .detail-section p { font-size:12px; line-height:1.7; white-space:pre-wrap; overflow-wrap:anywhere; } .source-link { display:flex; align-items:center; gap:6px; font-size:11px; padding:18px 0; }
  .run { display:flex; flex-direction:column; gap:5px; width:100%; padding:14px 0; text-align:left; border-bottom:1px solid rgb(var(--theme-border) / .2); font-size:12px; } pre { margin-top:16px; white-space:pre-wrap; overflow-wrap:anywhere; font-size:11px; line-height:1.6; } .notice { padding:10px 16px; color:rgb(var(--theme-text-secondary)); font-size:11px; overflow-wrap:anywhere; } .notice button { text-decoration:underline; } .empty { padding:72px 24px; text-align:center; color:rgb(var(--theme-text-tertiary)); } .empty :global(svg) { margin:auto; } .empty p { font-size:12px; line-height:1.7; } .empty h2 { font-size:16px; margin:16px 0 8px; }
  @media(max-width:1000px) { .body.has-selection { grid-template-columns:minmax(0,1fr) 280px; } .column-head,.automation-row { grid-template-columns:minmax(120px,1fr) 110px; gap:10px; } .column-head span:last-child,.last { display:none; } .page-head,.list-toolbar { padding-left:18px; padding-right:18px; } }
  @media(max-width:700px) { .body.has-selection { grid-template-columns:1fr; overflow:auto; } .catalog { min-height:280px; max-height:50vh; } aside { border-left:0; border-top:1px solid rgb(var(--theme-border) / .3); min-height:340px; } .page-head { flex-wrap:wrap; padding-top:18px; } .head-actions { flex-wrap:wrap; } .search { margin-left:0; flex:1; } input { width:100%; } .list-toolbar { gap:10px; } }
</style>
