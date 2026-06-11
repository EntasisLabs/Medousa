<script lang="ts">
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import {
    BACKEND_OPTIONS,
    DEPTH_OPTIONS,
    HOST_TURN_BUS_OPTIONS,
    TOOL_CALL_MODE_OPTIONS,
    WEB_SEARCH_PROVIDER_OPTIONS,
    WORKSHOP_DEFAULTS_TABS,
    type WorkshopDefaultsTab,
  } from "$lib/types/workshopDefaults";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
  }

  let { visible, mobile = false, embedded = false }: Props = $props();

  const visibleTabs = $derived(
    mobile
      ? [{ id: "setup" as WorkshopDefaultsTab, label: "Workshop" }]
      : WORKSHOP_DEFAULTS_TABS,
  );

  const policyOptions = ["balanced", "strict", "analytical", "fast"];

  $effect(() => {
    if (visible && !workshopDefaults.loaded) {
      void workshopDefaults.load();
    }
  });

  $effect(() => {
    if (
      mobile &&
      !visibleTabs.some((tab) => tab.id === workshopDefaults.activeTab)
    ) {
      workshopDefaults.activeTab = "setup";
    }
  });

  function numField(
    key: keyof typeof workshopDefaults.draft,
    event: Event,
  ) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function textField(
    key: keyof typeof workshopDefaults.draft,
    event: Event,
  ) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: (event.currentTarget as HTMLInputElement).value,
    };
  }

  function boolField(
    key: keyof typeof workshopDefaults.draft,
    event: Event,
  ) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: (event.currentTarget as HTMLInputElement).checked,
    };
  }
</script>

<section class="{embedded ? '' : 'workshop-inset p-3'}">
  {#if !embedded}
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h2 class="text-sm font-semibold text-surface-100">Workshop defaults</h2>
      <p class="workshop-faint mt-0.5">
        {#if mobile}
          Read-only snapshot from the Mac — day-to-day charter is in Settings → Memory & Voice.
        {:else}
          Terminal view of <span class="font-mono text-surface-400">tui_defaults.json</span> — day-to-day
          charter is in Settings; edit everything here when you need the full matrix.
        {/if}
      </p>
    </div>
    {#if !mobile}
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={workshopDefaults.saving || workshopDefaults.loading}
        onclick={() => workshopDefaults.save()}
      >
        {workshopDefaults.saving ? "Saving…" : "Save defaults"}
      </button>
    {/if}
  </div>
  {/if}

  {#if embedded && !mobile}
    <div class="mb-3 flex justify-end">
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={workshopDefaults.saving || workshopDefaults.loading}
        onclick={() => workshopDefaults.save()}
      >
        {workshopDefaults.saving ? "Saving…" : "Save defaults"}
      </button>
    </div>
  {/if}

  <div class="workshop-tabs {embedded ? 'mt-0' : 'mt-3'} flex-wrap">
    {#each visibleTabs as tab (tab.id)}
      <button
        type="button"
        class="workshop-tab {workshopDefaults.activeTab === tab.id
          ? 'workshop-tab-active'
          : ''}"
        onclick={() => (workshopDefaults.activeTab = tab.id as WorkshopDefaultsTab)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  {#if workshopDefaults.message}
    <p
      class="mt-2 text-xs {workshopDefaults.message.includes('saved')
        ? 'text-success-400'
        : 'text-warning-400'}"
    >
      {workshopDefaults.message}
    </p>
  {/if}

  {#if workshopDefaults.loading}
    <p class="workshop-faint mt-4 text-sm">Loading defaults…</p>
  {:else}
    <div class="mt-4 grid gap-4 {mobile ? '' : 'sm:grid-cols-2'}">
      {#if workshopDefaults.activeTab === "setup"}
        <label class="block">
          <span class="workshop-label">Backend</span>
          <select
            class="select mt-1 w-full"
            value={workshopDefaults.draft.backend ?? "surreal-mem"}
            onchange={(e) => textField("backend", e)}
          >
            {#each BACKEND_OPTIONS as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        </label>
        <label class="block">
          <span class="workshop-label">Provider</span>
          <input
            class="input mt-1 w-full"
            value={workshopDefaults.draft.provider ?? ""}
            readonly={mobile}
            disabled={mobile}
            oninput={(e) => textField("provider", e)}
          />
        </label>
        <label class="block">
          <span class="workshop-label">Model</span>
          <input
            class="input mt-1 w-full"
            value={workshopDefaults.draft.model ?? ""}
            readonly={mobile}
            disabled={mobile}
            oninput={(e) => textField("model", e)}
          />
        </label>
        <label class="block">
          <span class="workshop-label">Base URL</span>
          <input
            class="input mt-1 w-full"
            placeholder="optional provider endpoint"
            value={workshopDefaults.draft.baseUrl ?? ""}
            readonly={mobile}
            disabled={mobile}
            oninput={(e) => textField("baseUrl", e)}
          />
        </label>
        <label class="block sm:col-span-2">
          <span class="workshop-label">Allowed tools (comma-separated)</span>
          <textarea
            class="textarea mt-1 w-full font-mono text-xs"
            rows="2"
            bind:value={workshopDefaults.allowedModulesText}
            placeholder="websearch.search, fetch.url"
          ></textarea>
        </label>
        <div class="sm:col-span-2">
          <span class="workshop-label">Response depth</span>
          <div class="mt-2 flex flex-wrap gap-2">
            {#each DEPTH_OPTIONS as mode (mode)}
              <button
                type="button"
                class="rounded-container-token px-3 py-1.5 text-sm transition {workshopDefaults
                  .draft.responseDepthMode === mode
                  ? 'bg-primary-500/20 font-medium text-primary-200'
                  : 'bg-surface-800 text-surface-300 hover:text-surface-100'}"
                onclick={() =>
                  (workshopDefaults.draft = {
                    ...workshopDefaults.draft,
                    responseDepthMode: mode,
                  })}
              >
                {mode}
              </button>
            {/each}
          </div>
        </div>
      {:else if workshopDefaults.activeTab === "tools"}
        <label class="block">
          <span class="workshop-label">Tool call mode</span>
          <select
            class="select mt-1 w-full"
            value={workshopDefaults.draft.toolCallMode ?? "auto"}
            onchange={(e) => textField("toolCallMode", e)}
          >
            {#each TOOL_CALL_MODE_OPTIONS as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        </label>
        <label class="block">
          <span class="workshop-label">Host turn bus mode</span>
          <select
            class="select mt-1 w-full"
            value={workshopDefaults.draft.hostTurnBusMode ?? "auto"}
            onchange={(e) => textField("hostTurnBusMode", e)}
          >
            {#each HOST_TURN_BUS_OPTIONS as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        </label>
        <label class="block">
          <span class="workshop-label">Web search provider</span>
          <select
            class="select mt-1 w-full"
            value={workshopDefaults.draft.webSearchPreferredProvider ?? ""}
            disabled={mobile}
            onchange={(e) => textField("webSearchPreferredProvider", e)}
          >
            {#each WEB_SEARCH_PROVIDER_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
          <p class="workshop-faint mt-1 text-xs">
            Used by <span class="font-mono">cognition_web_search</span> before fallback bindings.
          </p>
        </label>
        <label class="flex cursor-pointer items-center gap-3 sm:col-span-2">
          <input
            type="checkbox"
            checked={workshopDefaults.draft.webSearchTryFallbacks ?? true}
            disabled={mobile}
            onchange={(e) => boolField("webSearchTryFallbacks", e)}
          />
          <span class="text-sm text-surface-200">
            Try other web providers when the preferred one fails
          </span>
        </label>
        {#each [
          ["Max tool rounds", "maxToolRounds"],
          ["Host bus max tool rounds", "hostBusMaxToolRounds"],
          ["Activation tool intent max rounds", "activationToolIntentMaxRounds"],
          ["Activation short turn max rounds", "activationShortTurnMaxToolRounds"],
          ["Continuation max tool rounds", "continuationMaxToolRounds"],
          ["Max text-only stuck continues", "maxTextOnlyStuckContinues"],
          ["Classifier restricted max rounds", "classifierRestrictedMaxToolRounds"],
        ] as row (row[1])}
          <label class="block">
            <span class="workshop-label">{row[0]}</span>
            <input
              type="number"
              class="input mt-1 w-full"
              value={workshopDefaults.draft[row[1] as keyof typeof workshopDefaults.draft] ?? ""}
              oninput={(e) => numField(row[1] as keyof typeof workshopDefaults.draft, e)}
            />
          </label>
        {/each}
      {:else if workshopDefaults.activeTab === "memory"}
        {#each [
          ["Direct answer max prompt chars", "activationDirectAnswerMaxPromptChars"],
          ["Long session turn threshold", "activationLongSessionTurnThreshold"],
          ["Long session max prompt chars", "activationLongSessionMaxPromptChars"],
          ["Slice hot window turns", "sliceHotWindowTurns"],
          ["Slice cold window turns", "sliceColdWindowTurns"],
        ] as row (row[1])}
          <label class="block">
            <span class="workshop-label">{row[0]}</span>
            <input
              type="number"
              class="input mt-1 w-full"
              value={workshopDefaults.draft[row[1] as keyof typeof workshopDefaults.draft] ?? ""}
              oninput={(e) => numField(row[1] as keyof typeof workshopDefaults.draft, e)}
            />
          </label>
        {/each}
      {:else if workshopDefaults.activeTab === "diagnostics"}
        <label class="flex cursor-pointer items-center gap-3 sm:col-span-2">
          <input
            type="checkbox"
            class="checkbox"
            checked={workshopDefaults.draft.thinkingCapture ?? true}
            onchange={(e) => boolField("thinkingCapture", e)}
          />
          <span class="text-sm text-surface-200">Capture thinking traces</span>
        </label>
        <label class="flex cursor-pointer items-center gap-3 sm:col-span-2">
          <input
            type="checkbox"
            class="checkbox"
            checked={workshopDefaults.draft.stasisOtelEnabled ?? false}
            onchange={(e) => boolField("stasisOtelEnabled", e)}
          />
          <span class="text-sm text-surface-200">Stasis OTEL export</span>
        </label>
        <label class="block">
          <span class="workshop-label">Thinking max lines</span>
          <input
            type="number"
            class="input mt-1 w-full"
            value={workshopDefaults.draft.thinkingMaxLines ?? 300}
            oninput={(e) => numField("thinkingMaxLines", e)}
          />
        </label>
      {:else if workshopDefaults.activeTab === "quality"}
        {#each [
          ["Retry runtime max retries", "retryRuntimeMaxRetries"],
          ["Retry runtime max rounds", "retryRuntimeMaxRounds"],
        ] as row (row[1])}
          <label class="block">
            <span class="workshop-label">{row[0]}</span>
            <input
              type="number"
              class="input mt-1 w-full"
              value={workshopDefaults.draft[row[1] as keyof typeof workshopDefaults.draft] ?? ""}
              oninput={(e) => numField(row[1] as keyof typeof workshopDefaults.draft, e)}
            />
          </label>
        {/each}
        {#each [
          ["Verifier min citation coverage", "verifierMinCitationCoverage"],
          ["Verifier min avg support strength", "verifierMinAvgSupportStrength"],
          ["Verifier min supported claim ratio", "verifierMinSupportedClaimRatio"],
          ["Verifier min claim support strength", "verifierMinClaimSupportStrength"],
        ] as row (row[1])}
          <label class="block">
            <span class="workshop-label">{row[0]}</span>
            <input
              type="number"
              step="0.05"
              min="0"
              max="1"
              class="input mt-1 w-full"
              value={workshopDefaults.draft[row[1] as keyof typeof workshopDefaults.draft] ?? ""}
              oninput={(e) => numField(row[1] as keyof typeof workshopDefaults.draft, e)}
            />
          </label>
        {/each}
      {:else if workshopDefaults.activeTab === "secrets"}
        <div class="sm:col-span-2">
          <span class="workshop-label">API key</span>
          <p class="workshop-faint mt-0.5 text-xs">
            {workshopDefaults.apiKeySet ? "A key is stored (keychain or file)." : "No key stored."}
          </p>
          <input
            type="password"
            class="input mt-2 w-full"
            placeholder="Enter new API key"
            bind:value={workshopDefaults.apiKeyDraft}
            disabled={workshopDefaults.clearApiKey}
          />
          <label class="mt-2 flex cursor-pointer items-center gap-2 text-sm text-surface-300">
            <input
              type="checkbox"
              class="checkbox"
              bind:checked={workshopDefaults.clearApiKey}
            />
            Clear stored API key on save
          </label>
        </div>
        <label class="block sm:col-span-2">
          <span class="workshop-label">Runtime env overrides</span>
          <textarea
            class="textarea mt-1 w-full font-mono text-xs"
            rows="4"
            placeholder="KEY=value per line"
            value={workshopDefaults.draft.envOverrides ?? ""}
            oninput={(e) => textField("envOverrides", e)}
          ></textarea>
        </label>
      {:else if workshopDefaults.activeTab === "specialists"}
        <label class="block sm:col-span-2">
          <span class="workshop-label">Stage role</span>
          <select
            class="select mt-1 w-full max-w-xs"
            bind:value={workshopDefaults.selectedRouteRole}
          >
            {#each workshopDefaults.routeRoles() as role (role)}
              <option value={role}>{role}</option>
            {/each}
          </select>
        </label>
        {#if workshopDefaults.selectedRoute()}
          {@const route = workshopDefaults.selectedRoute()!}
          <label class="block">
            <span class="workshop-label">Provider</span>
            <input
              class="input mt-1 w-full"
              value={route.provider}
              oninput={(e) =>
                workshopDefaults.updateSelectedRoute({
                  provider: (e.currentTarget as HTMLInputElement).value,
                })}
            />
          </label>
          <label class="block">
            <span class="workshop-label">Model</span>
            <input
              class="input mt-1 w-full"
              value={route.model}
              oninput={(e) =>
                workshopDefaults.updateSelectedRoute({
                  model: (e.currentTarget as HTMLInputElement).value,
                })}
            />
          </label>
          <label class="block">
            <span class="workshop-label">Policy profile</span>
            <select
              class="select mt-1 w-full"
              value={route.policy_profile}
              onchange={(e) =>
                workshopDefaults.updateSelectedRoute({
                  policy_profile: (e.currentTarget as HTMLSelectElement).value,
                })}
            >
              {#each policyOptions as policy (policy)}
                <option value={policy}>{policy}</option>
              {/each}
            </select>
          </label>
          <label class="block sm:col-span-2">
            <span class="workshop-label">Fallback chain (comma-separated)</span>
            <input
              class="input mt-1 w-full font-mono text-xs"
              value={route.fallback_chain.join(", ")}
              oninput={(e) =>
                workshopDefaults.updateSelectedRoute({
                  fallback_chain: (e.currentTarget as HTMLInputElement).value
                    .split(",")
                    .map((entry) => entry.trim())
                    .filter(Boolean),
                })}
            />
          </label>
        {/if}
      {/if}
    </div>
  {/if}
</section>
