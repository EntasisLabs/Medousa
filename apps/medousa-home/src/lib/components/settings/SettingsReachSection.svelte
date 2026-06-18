<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import {
    HOST_BUS_CHARTER_OPTIONS,
    STAGE_ROLE_LABELS,
    TOOL_CALL_CHARTER_OPTIONS,
  } from "$lib/types/settings";
  import { WEB_SEARCH_PROVIDER_OPTIONS } from "$lib/types/workshopDefaults";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const routeRows = $derived.by(() => {
    const matrix = workshopDefaults.draft.stageRouting;
    if (!matrix) return [];
    return workshopDefaults
      .routeRoles()
      .map((role) => {
        const route = matrix[role as keyof typeof matrix];
        if (!route) return null;
        return {
          role,
          label: STAGE_ROLE_LABELS[role] ?? role,
          target: `${route.provider}:${route.model}`,
          policy: route.policy_profile,
        };
      })
      .filter((row): row is NonNullable<typeof row> => row != null);
  });

  function numField(key: "maxToolRounds", event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function selectField(
    key: "toolCallMode" | "hostTurnBusMode",
    value: string,
  ) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: value,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Reach</h2>
    <p class="workshop-faint mt-1 text-sm">
      What Medousa is allowed to touch — and how she delegates when work gets heavy.
    </p>
  </header>

  <div class="mt-5">
    <label class="block">
      <span class="block text-sm font-medium text-surface-100">Allowed tools</span>
      <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">
        Comma-separated module names — leave empty to allow the full catalog. Channel tokens and
        delivery live in Messaging, not here.
      </span>
      <textarea
        class="textarea mt-2 w-full font-mono text-xs"
        rows="3"
        bind:value={workshopDefaults.allowedModulesText}
        placeholder="websearch.search, fetch.url"
        readonly={readOnly}
        disabled={readOnly}
      ></textarea>
    </label>
  </div>

  <div class="mt-6 grid gap-4 sm:grid-cols-2">
    <label class="block">
      <span class="block text-sm font-medium text-surface-100">Web search</span>
      <span class="workshop-faint mt-0.5 block text-xs">Preferred provider when she looks things up</span>
      <select
        class="select mt-2 w-full"
        value={workshopDefaults.draft.webSearchPreferredProvider ?? ""}
        disabled={readOnly}
        onchange={(event) =>
          (workshopDefaults.draft = {
            ...workshopDefaults.draft,
            webSearchPreferredProvider: (event.currentTarget as HTMLSelectElement).value,
          })}
      >
        {#each WEB_SEARCH_PROVIDER_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
    <label class="settings-toggle-row mt-6 sm:mt-0 sm:flex sm:flex-col sm:justify-end sm:border-0 sm:bg-transparent sm:px-0 sm:py-0">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Search fallbacks</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          Try other providers when the preferred one fails
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={workshopDefaults.draft.webSearchTryFallbacks ?? true}
        disabled={readOnly}
        onchange={(event) =>
          (workshopDefaults.draft = {
            ...workshopDefaults.draft,
            webSearchTryFallbacks: (event.currentTarget as HTMLInputElement).checked,
          })}
      />
    </label>
  </div>

  <div class="mt-6">
    <span class="block text-sm font-medium text-surface-100">Tool posture</span>
    <span class="workshop-faint mt-0.5 block text-xs">How strictly she invokes tools on a turn</span>
    <div class="mt-3 grid gap-2 sm:grid-cols-2">
      {#each TOOL_CALL_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.toolCallMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          onclick={() => selectField("toolCallMode", option.id)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <span class="block text-sm font-medium text-surface-100">When to bring in Specialists</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      How often she routes a turn through specialist models
    </span>
    <div class="mt-3 grid gap-2 sm:grid-cols-3">
      {#each HOST_BUS_CHARTER_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {workshopDefaults.draft.hostTurnBusMode === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          onclick={() => selectField("hostTurnBusMode", option.id)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  <label class="mt-6 block max-w-xs">
    <span class="block text-sm font-medium text-surface-100">Tool rounds per turn</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      How many tool calls she may chain before stopping on one turn
    </span>
    <input
      type="number"
      class="input mt-2 w-full"
      min="1"
      max="24"
      value={workshopDefaults.draft.maxToolRounds ?? 10}
      readonly={readOnly}
      disabled={readOnly}
      oninput={(event) => numField("maxToolRounds", event)}
    />
  </label>

  {#if routeRows.length > 0}
    <div class="mt-8">
      <div class="flex flex-wrap items-end justify-between gap-2">
        <div>
          <h3 class="text-sm font-semibold text-surface-100">Routing posture</h3>
          <p class="workshop-faint mt-0.5 text-xs">
            Who handles each stage — saved with your charter
          </p>
        </div>
      </div>
      <div class="mt-3 overflow-x-auto rounded-container-token border border-surface-500/35">
        <table class="w-full min-w-[28rem] text-left text-xs">
          <thead>
            <tr class="workshop-label border-b border-surface-500/35 bg-surface-900/40">
              <th class="px-3 py-2 font-medium">Stage</th>
              <th class="px-3 py-2 font-medium">Model</th>
              <th class="px-3 py-2 font-medium">Policy</th>
            </tr>
          </thead>
          <tbody>
            {#each routeRows as row (row.role)}
              <tr class="border-b border-surface-500/10 last:border-0">
                <td class="px-3 py-2.5 font-medium text-surface-200">{row.label}</td>
                <td class="px-3 py-2.5 font-mono text-surface-300">{row.target}</td>
                <td class="px-3 py-2.5 text-surface-300">{row.policy}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <p class="workshop-faint mt-2 text-xs">
        Edit individual specialist models in Runtime → Workshop → Specialists. Runtime → Routing
        shows live telemetry of what is configured.
      </p>
    </div>
  {/if}

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
