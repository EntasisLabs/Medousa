<script lang="ts">
  import { onMount } from "svelte";
  import { daemonUnary } from "$lib/daemon/contractClient";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ExternalAgentExecutor, ExternalPeerRuntime } from "$lib/types/generated/daemon_api";
  import type { AgentRuntimeListResponse } from "$lib/daemon/session";
  import type { ItemProjection } from "$lib/forge";

  let { executor = $bindable(null), disabled = false }: { executor?: ExternalAgentExecutor | null; disabled?: boolean } = $props();
  type Project = ItemProjection & { environment?: ItemProjection["environment"] & { repo?: { repo_id: string } } };
  let runtime = $state<"medousa" | ExternalPeerRuntime>(executor?.runtime ?? "medousa");
  let workshopId = $state(executor?.home_workshop_id ?? "");
  let projects = $state<Project[]>([]);
  let runtimes = $state<AgentRuntimeListResponse["runtimes"]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let sequence = 0;
  const targets = $derived(executionTargets.userTargets());
  onMount(() => { void executionTargets.refresh().catch(() => undefined); });
  $effect(() => {
    if (runtime !== "medousa" && !workshopId && executionTargets.inventory) workshopId = executionTargets.inventory.parent_runtime_id;
  });
  $effect(() => {
    const target = workshopId;
    const scope = chat.workshopScopeId;
    const request = ++sequence;
    projects = []; runtimes = []; error = null; loading = false;
    if (runtime === "medousa" || !target) return;
    loading = true;
    void Promise.all([
      loadProjects(target),
      daemonUnary<AgentRuntimeListResponse>("agents.runtimes.get", {}, undefined, target),
    ]).then(([result, agents]) => {
      if (sequence !== request || scope !== chat.workshopScopeId) return;
      projects = result.filter((item) => item.workspace_present && item.environment?.repo?.repo_id);
      runtimes = agents.runtimes;
    }).catch((cause) => { if (sequence === request) error = cause instanceof Error ? cause.message : String(cause); })
      .finally(() => { if (sequence === request) loading = false; });
  });
  async function loadProjects(target: string): Promise<Project[]> {
    const all: Project[] = [];
    const seen = new Set<string>();
    let cursor: string | null = null;
    do {
      const query: Record<string, string> = { limit: "256" };
      if (cursor) query.cursor = cursor;
      const page: Project[] | { items: Project[]; truncated?: boolean; next_cursor?: string | null } = await daemonUnary("forge.items.get", {}, undefined, target, query);
      if (Array.isArray(page)) return page;
      all.push(...page.items);
      cursor = page.truncated ? page.next_cursor ?? null : null;
      if (cursor && seen.has(cursor)) throw new Error("Project list returned a repeated cursor");
      if (cursor) seen.add(cursor);
    } while (cursor);
    return all;
  }
  function changeRuntime(value: "medousa" | ExternalPeerRuntime) {
    runtime = value;
    if (value === "medousa") executor = null;
    else executor = { runtime: value, home_workshop_id: workshopId, forge_work_id: "", forge_repo_id: "", session_contract: "fresh_per_job", allowed_tools: [], allowed_capabilities: [] };
  }
  function changeWorkshop(value: string) {
    workshopId = value;
    if (executor) executor = { ...executor, home_workshop_id: value, forge_work_id: "", forge_repo_id: "" };
  }
  function chooseProject(id: string) {
    const project = projects.find((item) => item.id === id);
    if (executor) executor = { ...executor, home_workshop_id: workshopId, forge_work_id: id, forge_repo_id: project?.environment?.repo?.repo_id ?? "" };
  }
  const unavailable = $derived(runtime !== "medousa" ? runtimes.find((item) => item.runtime === runtime && !item.available) : undefined);
</script>

<div class="configuration">
  <label>Runs with<select aria-label="Bot runtime" value={runtime} disabled={disabled} onchange={(event) => changeRuntime(event.currentTarget.value as typeof runtime)}><option value="medousa">Medousa</option><option value="codex">Codex</option><option value="cursor">Cursor</option><option value="hermes">Hermes</option></select></label>
  {#if runtime !== "medousa"}
    <p>This runtime stays with your Bot. Each request runs in its pinned project.</p>
    <label>Workshop<select aria-label="Bot workshop" value={workshopId} disabled={disabled} onchange={(event) => changeWorkshop(event.currentTarget.value)}><option value="">Choose workshop</option>{#each targets as target (target.runtime_id)}<option value={target.runtime_id}>{target.label}</option>{/each}{#if workshopId && !targets.some((target) => target.runtime_id === workshopId)}<option value={workshopId}>{workshopId}</option>{/if}</select></label>
    <label>Project<select aria-label="Bot project" value={executor?.forge_work_id ?? ""} disabled={disabled || loading} onchange={(event) => chooseProject(event.currentTarget.value)}><option value="">{loading ? "Loading projects…" : "Choose a provisioned project"}</option>{#each projects as project (project.id)}<option value={project.id}>{project.title}</option>{/each}{#if executor?.forge_work_id && !projects.some((project) => project.id === executor?.forge_work_id)}<option value={executor.forge_work_id}>Current project · {executor.forge_work_id}</option>{/if}</select></label>
    {#if !loading && !error && projects.length === 0}<p>Create and provision a project in Code on this workshop, then return here.</p>{/if}
    {#if unavailable}<p class="error">{unavailable.detail || `${runtime} is unavailable on this workshop. Install it in Settings → Packages.`}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  {/if}
</div>
<style>
  .configuration { display: flex; flex-direction: column; gap: 12px; }
  label { font-size: 12px; font-weight: 500; }
  select { display: block; width: 100%; margin-top: 8px; padding: 11px 12px; border: 1px solid rgb(var(--theme-border) / .5); border-radius: 11px; background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); font-size: 14px; }
  p { font-size: 12px; line-height: 1.5; color: rgb(var(--theme-text-secondary)); }
  .error { color: #f3a1ad; }
</style>
