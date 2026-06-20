<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
  import type { UpdateManuscriptRequest } from "$lib/types/manuscript";

  interface Props {
    entry: ManuscriptCatalogEntry;
    onRunSkill: (manuscriptId: string) => void;
    onUseInAutomation: (entry: ManuscriptCatalogEntry) => void;
    onScheduleSkill: (entry: ManuscriptCatalogEntry) => void;
    onOpenFile: (path: string) => void;
  }

  let {
    entry,
    onRunSkill,
    onUseInAutomation,
    onScheduleSkill,
    onOpenFile,
  }: Props = $props();

  let showAdvanced = $state(false);
  let toolSearch = $state("");
  let taskTemplate = $state("");
  let scheduleCron = $state("");
  let scheduleExecutionMode = $state("agent_turn");
  let deliveryMode = $state("");
  let deliveryOnComplete = $state("");
  let toolsAllow = $state<string[]>([]);
  let openshellAllowScheduled = $state(false);

  const detail = $derived(catalog.manuscriptDetail);

  $effect(() => {
    if (!detail || detail.id !== entry.id) return;
    taskTemplate = detail.task_template ?? "";
    scheduleCron = detail.schedule_cron ?? "";
    scheduleExecutionMode = detail.schedule_execution_mode ?? "agent_turn";
    deliveryMode = detail.delivery_mode ?? "";
    deliveryOnComplete = detail.delivery_on_complete ?? "";
    toolsAllow = [...detail.tools_allow];
    openshellAllowScheduled = detail.openshell.allow_scheduled;
  });

  const filteredPalette = $derived(
    (detail?.palette_tools ?? []).filter((tool) => {
      const needle = toolSearch.trim().toLowerCase();
      if (!needle) return true;
      return tool.toLowerCase().includes(needle);
    }),
  );

  function toggleTool(tool: string, enabled: boolean) {
    if (enabled) {
      if (!toolsAllow.includes(tool)) {
        toolsAllow = [...toolsAllow, tool];
      }
    } else {
      toolsAllow = toolsAllow.filter((value) => value !== tool);
    }
  }

  async function saveChanges() {
    const request: UpdateManuscriptRequest = {
      task_template: taskTemplate.trim() || undefined,
      clear_task_template: taskTemplate.trim() ? undefined : true,
      tools_allow: toolsAllow,
      schedule_cron: scheduleCron.trim() || undefined,
      clear_schedule_cron: scheduleCron.trim() ? undefined : true,
      schedule_execution_mode: scheduleExecutionMode.trim() || undefined,
      delivery_mode: deliveryMode.trim() || undefined,
      delivery_on_complete: deliveryOnComplete.trim() || undefined,
      openshell_allow_scheduled: openshellAllowScheduled,
    };
    await catalog.saveManuscriptDetail(entry.id, request);
  }
</script>

<h2 class="workshop-section-title">Specialist detail</h2>
<p class="mt-2 font-medium text-surface-100">{entry.name}</p>
<p class="workshop-faint mt-1 font-mono text-[11px]">{entry.id}</p>

{#if catalog.manuscriptDetailLoading}
  <p class="workshop-muted mt-4 text-sm">Loading editor…</p>
{:else if catalog.manuscriptDetailError}
  <p class="mt-4 text-sm text-warning-400">{catalog.manuscriptDetailError}</p>
{:else if detail}
  {#if entry.description || detail.description}
    <p class="mt-3 text-sm leading-relaxed text-surface-300">
      {entry.description ?? detail.description}
    </p>
  {/if}

  <div class="mt-4 rounded-md border border-surface-500/35 px-3 py-2 text-xs">
    <div class="flex flex-wrap items-center gap-2">
      <span class="workshop-label">Schedule readiness</span>
      {#if detail.schedule_ready}
        <span class="text-[10px] uppercase tracking-wide text-primary-300">Ready</span>
      {:else}
        <span class="text-[10px] uppercase tracking-wide text-warning-400">Needs attention</span>
      {/if}
    </div>
    {#if detail.schedule_validation_error}
      <p class="mt-1 text-warning-400/90">{detail.schedule_validation_error}</p>
    {/if}
  </div>

  <label class="mt-4 block text-xs">
    <span class="workshop-label">Task template</span>
    <GrowingTextarea
      bind:value={taskTemplate}
      minHeight={72}
      maxHeight={160}
      placeholder="What this specialist does when scheduled or invoked…"
      aria-label="Task template"
    />
  </label>

  <div class="mt-4 grid gap-3">
    <label class="block text-xs">
      <span class="workshop-label">Default schedule (cron)</span>
      <input class="input mt-1 w-full font-mono text-sm" bind:value={scheduleCron} placeholder="0 9 * * *" />
    </label>
    <label class="block text-xs">
      <span class="workshop-label">Execution mode</span>
      <select class="input mt-1 w-full text-sm" bind:value={scheduleExecutionMode}>
        <option value="agent_turn">Agent turn (default)</option>
        <option value="prompt">Quick prompt only</option>
      </select>
    </label>
  </div>

  <div class="mt-4 grid gap-3">
    <label class="block text-xs">
      <span class="workshop-label">Delivery mode</span>
      <input class="input mt-1 w-full text-sm" bind:value={deliveryMode} placeholder="optional" />
    </label>
    <label class="block text-xs">
      <span class="workshop-label">On complete</span>
      <select class="input mt-1 w-full text-sm" bind:value={deliveryOnComplete}>
        <option value="">None</option>
        <option value="locus">Remember (Locus)</option>
        <option value="store">Store</option>
      </select>
    </label>
  </div>

  <div class="mt-4">
    <div class="flex items-center justify-between gap-2">
      <h3 class="workshop-label">Skills attached</h3>
      <span class="workshop-faint text-[11px]">{toolsAllow.length} selected</span>
    </div>
    <input
      class="input mt-2 w-full text-sm"
      type="search"
      placeholder="Filter tools…"
      bind:value={toolSearch}
    />
    <ul class="mt-2 max-h-40 space-y-1 overflow-y-auto">
      {#each filteredPalette as tool (tool)}
        <li>
          <label class="flex items-center gap-2 rounded px-1 py-1 text-[11px] text-surface-300 hover:bg-surface-800/60">
            <input
              type="checkbox"
              class="checkbox"
              checked={toolsAllow.includes(tool)}
              onchange={(event) =>
                toggleTool(tool, (event.currentTarget as HTMLInputElement).checked)}
            />
            <span class="font-mono">{tool}</span>
          </label>
        </li>
      {/each}
    </ul>
  </div>

  {#if detail.scheduled_tools.length > 0}
    <div class="mt-4">
      <h3 class="workshop-label">Scheduled lane preview</h3>
      <ul class="mt-2 max-h-36 space-y-1 overflow-y-auto">
        {#each detail.scheduled_tools as row (row.tool)}
          <li class="rounded-md border border-surface-500/30 px-2 py-1.5 text-[11px]">
            <div class="flex flex-wrap items-center gap-2">
              <span class="font-mono text-surface-200">{row.tool}</span>
              <span
                class="text-[10px] uppercase tracking-wide {row.allowed_on_schedule
                  ? 'text-primary-300'
                  : 'text-warning-400/80'}"
              >
                {row.allowed_on_schedule ? "schedule ok" : "interactive only"}
              </span>
            </div>
            {#if row.reason}
              <p class="workshop-faint mt-0.5">{row.reason}</p>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if detail.openshell.enabled}
    <div class="mt-4 rounded-md border border-surface-500/35">
      <button
        type="button"
        class="flex w-full items-center justify-between px-3 py-2 text-left text-xs"
        onclick={() => (showAdvanced = !showAdvanced)}
      >
        <span class="workshop-label">Advanced · OpenShell sandbox</span>
        <span class="text-surface-500">{showAdvanced ? "−" : "+"}</span>
      </button>
      {#if showAdvanced}
        <div class="border-t border-surface-500/35 px-3 py-3 text-xs text-surface-300">
          <p>
            Default automation path:
            <span class="text-surface-200">{detail.openshell.default_path}</span>
          </p>
          {#if detail.openshell.policy_template}
            <p class="mt-2 font-mono text-[11px]">
              Policy: {detail.openshell.policy_template}
            </p>
          {/if}
          {#if detail.openshell.sandbox_from}
            <p class="mt-1 font-mono text-[11px]">
              Sandbox: {detail.openshell.sandbox_from}
            </p>
          {/if}
          <label class="mt-3 flex items-center gap-2">
            <input type="checkbox" class="checkbox" bind:checked={openshellAllowScheduled} />
            Allow OpenShell tools on scheduled runs
          </label>
        </div>
      {/if}
    </div>
  {:else}
    <p class="workshop-faint mt-4 text-xs">
      Grapheme is the default skill path. OpenShell appears here when this specialist ships scripts.
    </p>
  {/if}

  {#if entry.scripts.length > 0}
    <div class="mt-4">
      <h3 class="workshop-label">Scripts</h3>
      <ul class="mt-2 space-y-1 text-[11px] text-surface-300">
        {#each entry.scripts as script (script.relative_path)}
          <li class="font-mono">
            {script.relative_path}
            <span class="text-surface-500">({script.risk_class})</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="mt-4 flex flex-wrap gap-3">
    <button
      type="button"
      class="btn btn-sm variant-filled-primary"
      disabled={catalog.manuscriptSaveBusy}
      onclick={() => void saveChanges()}
    >
      {catalog.manuscriptSaveBusy ? "Saving…" : "Save changes"}
    </button>
    {#if catalog.manuscriptSaveMessage}
      <span class="self-center text-xs text-surface-400">{catalog.manuscriptSaveMessage}</span>
    {/if}
  </div>

  <div class="mt-5 flex flex-wrap gap-3">
    {#if entry.has_scripts}
      <button type="button" class="workshop-text-action" onclick={() => onRunSkill(entry.id)}>
        Run in chat
      </button>
    {/if}
    <button type="button" class="workshop-text-action" onclick={() => onUseInAutomation(entry)}>
      Use in automation
    </button>
    <button type="button" class="workshop-text-action" onclick={() => onScheduleSkill(entry)}>
      Schedule…
    </button>
    <button type="button" class="workshop-text-action" onclick={() => onOpenFile(entry.path)}>
      Open YAML
    </button>
  </div>
{/if}
