<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import SettingsVersionsSection from "$lib/components/settings/SettingsVersionsSection.svelte";
  import SettingsWorkerCapacity from "$lib/components/settings/SettingsWorkerCapacity.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauri } from "$lib/window";
  import {
    HOST_BUS_CHARTER_OPTIONS,
    TOOL_CALL_CHARTER_OPTIONS,
  } from "$lib/types/settings";
  import {
    BACKEND_OPTIONS,
    WEB_SEARCH_PROVIDER_OPTIONS,
    listToMultilineText,
    parseMultilineList,
  } from "$lib/types/workshopDefaults";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    nativeWorkloads?: boolean;
  }

  let { nativeWorkloads = true }: Props = $props();

  type Picker = "posture" | "specialists" | "search" | "backend" | null;

  const BACKEND_LABELS: Record<(typeof BACKEND_OPTIONS)[number], { label: string; hint: string }> = {
    "surreal-mem": {
      label: "Surreal (memory)",
      hint: "Default — fast in-process store",
    },
    "in-memory": {
      label: "In-memory",
      hint: "Ephemeral — cleared when the engine stops",
    },
    "surreal-kv": {
      label: "Surreal (disk)",
      hint: "Persists across restarts",
    },
  };

  const toolBudgetFields = [
    {
      key: "hostBusMaxToolRounds" as const,
      label: "Specialist rounds",
      hint: "Tool calls helpers may make in one turn",
      unit: "rounds",
      min: 1,
      max: 48,
    },
    {
      key: "activationToolIntentMaxRounds" as const,
      label: "Heavy-turn budget",
      hint: "Extra room when a turn clearly needs tools",
      unit: "rounds",
      min: 1,
      max: 48,
    },
    {
      key: "activationShortTurnMaxToolRounds" as const,
      label: "Short-turn budget",
      hint: "Fewer calls when the turn looks light",
      unit: "rounds",
      min: 1,
      max: 24,
    },
    {
      key: "continuationMaxToolRounds" as const,
      label: "Follow-up budget",
      hint: "Tool calls on continuation turns",
      unit: "rounds",
      min: 1,
      max: 48,
    },
    {
      key: "maxTextOnlyStuckContinues" as const,
      label: "Stuck-turn retries",
      hint: "Nudges for a stuck text-only turn",
      unit: "tries",
      min: 0,
      max: 12,
    },
    {
      key: "classifierRestrictedMaxToolRounds" as const,
      label: "Restricted budget",
      hint: "When reach is tightened for safety",
      unit: "rounds",
      min: 0,
      max: 24,
    },
  ];

  const qualityRetryFields = [
    {
      key: "retryRuntimeMaxRetries" as const,
      label: "Max retries",
      hint: "Retries after a failed step",
      unit: "tries",
      min: 0,
      max: 12,
      step: 1,
      wide: false,
    },
    {
      key: "retryRuntimeMaxRounds" as const,
      label: "Retry rounds",
      hint: "Round budget while retrying",
      unit: "rounds",
      min: 0,
      max: 48,
      step: 1,
      wide: false,
    },
  ];

  const qualityVerifierFields = [
    {
      key: "verifierMinCitationCoverage" as const,
      label: "Citation coverage",
      hint: "Minimum share of claims that need citations",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
    {
      key: "verifierMinAvgSupportStrength" as const,
      label: "Avg support",
      hint: "Minimum average support strength",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
    {
      key: "verifierMinSupportedClaimRatio" as const,
      label: "Supported claims",
      hint: "Minimum ratio of claims that pass support",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
    {
      key: "verifierMinClaimSupportStrength" as const,
      label: "Claim support",
      hint: "Minimum support for one claim",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
  ];

  let picker = $state<Picker>(null);
  let toolsOpen = $state(false);
  let allowlistsOpen = $state(false);
  let budgetsOpen = $state(false);
  let qualityOpen = $state(false);
  let hostOpen = $state(false);

  let binariesText = $state("");
  let writableRootsText = $state("");
  let syncedFrom = $state<string | null>(null);

  const agentToolsOn = $derived(workshopDefaults.draft.shellAgentToolsEnabled ?? false);
  const networkOn = $derived(workshopDefaults.draft.shellNetworkDefault ?? false);
  const binariesEmpty = $derived(parseMultilineList(binariesText).length === 0);
  const modulesEmpty = $derived(workshopDefaults.allowedModulesText.trim().length === 0);

  const activePosture = $derived(
    TOOL_CALL_CHARTER_OPTIONS.find(
      (option) => option.id === (workshopDefaults.draft.toolCallMode ?? "auto"),
    ) ?? TOOL_CALL_CHARTER_OPTIONS[0]!,
  );
  const activeSpecialists = $derived(
    HOST_BUS_CHARTER_OPTIONS.find(
      (option) => option.id === (workshopDefaults.draft.hostTurnBusMode ?? "auto"),
    ) ?? HOST_BUS_CHARTER_OPTIONS[0]!,
  );
  const preferredProvider = $derived(workshopDefaults.draft.webSearchPreferredProvider ?? "");
  const activeSearch = $derived(
    WEB_SEARCH_PROVIDER_OPTIONS.find((option) => option.value === preferredProvider) ??
      WEB_SEARCH_PROVIDER_OPTIONS[0]!,
  );
  const activeBackend = $derived(
    (workshopDefaults.draft.backend ?? "surreal-mem") as (typeof BACKEND_OPTIONS)[number],
  );
  const activeBackendMeta = $derived(
    BACKEND_LABELS[activeBackend] ?? BACKEND_LABELS["surreal-mem"],
  );

  const runtimeSummary = $derived.by(() => {
    const rounds = workshopDefaults.draft.maxToolRounds ?? 30;
    const native = nativeWorkloads
      ? ` · ${agentToolsOn ? "Shell on" : "Shell off"} · ${workshopDefaults.draft.vaultGitEnabled ? "Versions on" : "Versions off"}`
      : "";
    return `${activePosture.label} · ${activeSpecialists.label}${native} · ${rounds} rounds`;
  });

  $effect(() => {
    if (!workshopDefaults.loaded) return;
    const fingerprint = JSON.stringify([
      workshopDefaults.draft.shellAllowedBinaries ?? [],
      workshopDefaults.draft.shellWritableRoots ?? [],
    ]);
    if (syncedFrom === fingerprint) return;
    binariesText = listToMultilineText(workshopDefaults.draft.shellAllowedBinaries);
    writableRootsText = listToMultilineText(workshopDefaults.draft.shellWritableRoots);
    syncedFrom = fingerprint;
  });

  $effect(() => {
    if (!nativeWorkloads || !workshopDefaults.loaded) return;
    void workshop.loadAllowlist();
  });

  function togglePicker(next: Picker) {
    picker = picker === next ? null : next;
  }

  function selectField(key: "toolCallMode" | "hostTurnBusMode", value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: value,
    };
    picker = null;
  }

  function setWebSearchProvider(value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      webSearchPreferredProvider: value,
    };
    picker = null;
  }

  function setBackend(value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      backend: value,
    };
    picker = null;
  }

  function numField(key: keyof typeof workshopDefaults.draft, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function setTimeoutMs(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellTimeoutMs: Number.isFinite(value) ? Math.max(100, Math.round(value)) : 30_000,
    };
  }

  function setMaxOutput(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellMaxOutputBytes: Number.isFinite(value)
        ? Math.max(1024, Math.round(value))
        : 262_144,
    };
  }

  function setEnvOverrides(event: Event) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      envOverrides: (event.currentTarget as HTMLTextAreaElement).value,
    };
  }

  function syncListsIntoDraft() {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellAllowedBinaries: parseMultilineList(binariesText),
      shellWritableRoots: parseMultilineList(writableRootsText),
    };
    // Keep the draft→textarea effect from rewriting mid-edit.
    syncedFrom = JSON.stringify([
      workshopDefaults.draft.shellAllowedBinaries ?? [],
      workshopDefaults.draft.shellWritableRoots ?? [],
    ]);
  }

  function onBinariesInput(event: Event) {
    binariesText = (event.currentTarget as HTMLTextAreaElement).value;
    syncListsIntoDraft();
  }

  function onWritableRootsInput(event: Event) {
    writableRootsText = (event.currentTarget as HTMLTextAreaElement).value;
    syncListsIntoDraft();
  }

  async function beforeSave(): Promise<boolean> {
    if (!nativeWorkloads) return true;
    syncListsIntoDraft();
    if (!(agentToolsOn && binariesEmpty)) return true;
    const warning =
      "Agent shell tools are on with an empty binary allowlist. Any command basename inside the jail can run. Save anyway?";
    // Prefer plugin `ask` (`dialog|message`). Do not use `window.confirm` in
    // Tauri — it routes to a removed `dialog.confirm` command and aborts save.
    if (!isTauri()) return window.confirm(warning);
    try {
      const { ask } = await import("@tauri-apps/plugin-dialog");
      return await ask(warning, { title: "Shell allowlist", kind: "warning" });
    } catch (err) {
      workshopDefaults.message =
        err instanceof Error
          ? err.message
          : "Could not confirm empty shell allowlist — save cancelled.";
      return false;
    }
  }
</script>

<section class="settings-section prefs runtime">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Runtime Controls</h2>
    <p class="workshop-faint mt-1 text-sm">{runtimeSummary}</p>
  </header>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Reach</h3>
      <p class="settings-subsection-lead">How she uses tools on a turn.</p>
    </div>

    <div class="prefs-stack">
      <div class="rt-active">
        <button
          type="button"
          class="rt-active-trigger"
          class:rt-active-trigger-open={picker === "posture"}
          aria-expanded={picker === "posture"}
          disabled={workshopDefaults.saving}
          onclick={() => togglePicker("posture")}
        >
          <span class="rt-active-copy">
            <span class="rt-active-kicker">Tool posture</span>
            <span class="rt-active-title">{activePosture.label}</span>
            <span class="rt-active-meta">{activePosture.hint}</span>
          </span>
          <span class="rt-active-action workshop-faint">
            {picker === "posture" ? "Close" : "Change"}
          </span>
        </button>
        {#if picker === "posture"}
          <div class="prefs-grid rt-picker" role="listbox" aria-label="Tool posture">
            {#each TOOL_CALL_CHARTER_OPTIONS as option (option.id)}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={activePosture.id === option.id}
                aria-selected={activePosture.id === option.id}
                disabled={workshopDefaults.saving}
                onclick={() => selectField("toolCallMode", option.id)}
              >
                <span class="prefs-choice-label">{option.label}</span>
                <span class="prefs-choice-hint">{option.hint}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="rt-active">
        <button
          type="button"
          class="rt-active-trigger"
          class:rt-active-trigger-open={picker === "specialists"}
          aria-expanded={picker === "specialists"}
          disabled={workshopDefaults.saving}
          onclick={() => togglePicker("specialists")}
        >
          <span class="rt-active-copy">
            <span class="rt-active-kicker">Specialists</span>
            <span class="rt-active-title">{activeSpecialists.label}</span>
            <span class="rt-active-meta">{activeSpecialists.hint}</span>
          </span>
          <span class="rt-active-action workshop-faint">
            {picker === "specialists" ? "Close" : "Change"}
          </span>
        </button>
        {#if picker === "specialists"}
          <div class="prefs-grid rt-picker" role="listbox" aria-label="Specialists">
            {#each HOST_BUS_CHARTER_OPTIONS as option (option.id)}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={activeSpecialists.id === option.id}
                aria-selected={activeSpecialists.id === option.id}
                disabled={workshopDefaults.saving}
                onclick={() => selectField("hostTurnBusMode", option.id)}
              >
                <span class="prefs-choice-label">{option.label}</span>
                <span class="prefs-choice-hint">{option.hint}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="rt-active">
        <button
          type="button"
          class="rt-active-trigger"
          class:rt-active-trigger-open={picker === "search"}
          aria-expanded={picker === "search"}
          disabled={workshopDefaults.saving}
          onclick={() => togglePicker("search")}
        >
          <span class="rt-active-copy">
            <span class="rt-active-kicker">Web search</span>
            <span class="rt-active-title">{activeSearch.label}</span>
            <span class="rt-active-meta">Preferred provider when she looks things up</span>
          </span>
          <span class="rt-active-action workshop-faint">
            {picker === "search" ? "Close" : "Change"}
          </span>
        </button>
        {#if picker === "search"}
          <div class="prefs-grid rt-picker" role="listbox" aria-label="Web search provider">
            {#each WEB_SEARCH_PROVIDER_OPTIONS as option (option.value)}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={preferredProvider === option.value}
                aria-selected={preferredProvider === option.value}
                disabled={workshopDefaults.saving}
                onclick={() => setWebSearchProvider(option.value)}
              >
                <span class="prefs-choice-label">{option.label}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="prefs-grid">
        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Search fallbacks</span>
            <span class="prefs-tile-meta">Try others when preferred fails</span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={workshopDefaults.draft.webSearchTryFallbacks ?? true}
            disabled={workshopDefaults.saving}
            onchange={(event) =>
              (workshopDefaults.draft = {
                ...workshopDefaults.draft,
                webSearchTryFallbacks: (event.currentTarget as HTMLInputElement).checked,
              })}
          />
        </label>

        <label class="prefs-tile prefs-tile-metric">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">General tool rounds</span>
            <span class="prefs-tile-meta">Coder uses a separate 100-round ceiling</span>
          </span>
          <span class="prefs-metric">
            <input
              type="number"
              class="prefs-metric-input"
              min="1"
              max="48"
              inputmode="numeric"
              value={workshopDefaults.draft.maxToolRounds ?? 30}
              disabled={workshopDefaults.saving}
              aria-label="General tool rounds per turn"
              oninput={(event) => numField("maxToolRounds", event)}
            />
            <span class="prefs-metric-unit">rounds</span>
          </span>
        </label>
      </div>
    </div>

    <details class="prefs-more" bind:open={toolsOpen}>
      <summary class="prefs-more-summary">
        <span class="prefs-more-summary-copy">
          <span>Allowed tools</span>
          <span class="prefs-more-summary-meta">
            {modulesEmpty ? "Full catalog" : "Custom list"}
          </span>
        </span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body">
        <p class="prefs-footnote mb-2">
          Module names she can call (for example
          <span class="font-mono text-content-secondary">websearch.search</span>).
        </p>
        <textarea
          class="rt-mono-input"
          rows="2"
          bind:value={workshopDefaults.allowedModulesText}
          placeholder="websearch.search, fetch.url"
        ></textarea>
        {#if modulesEmpty}
          <p class="settings-danger-callout mt-3 text-xs leading-relaxed" role="status">
            Empty list means the full tool catalog is allowed.
          </p>
        {/if}
      </div>
    </details>
  </div>

  {#if nativeWorkloads}
    <div class="prefs-band">
      <div class="prefs-band-head">
        <h3 class="settings-subsection-heading">Shell</h3>
        <p class="settings-subsection-lead">Process sandbox — locked down until you open it.</p>
      </div>

    <div class="prefs-grid">
      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Agent shell tools</span>
          <span class="prefs-tile-meta">Unlock cognition_shell_* for specialists</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={agentToolsOn}
          disabled={workshopDefaults.saving}
          onchange={(event) =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              shellAgentToolsEnabled: (event.currentTarget as HTMLInputElement).checked,
            })}
        />
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Network ceiling</span>
          <span class="prefs-tile-meta">Calls may opt in only if this is on</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={networkOn}
          disabled={workshopDefaults.saving}
          onchange={(event) =>
            (workshopDefaults.draft = {
              ...workshopDefaults.draft,
              shellNetworkDefault: (event.currentTarget as HTMLInputElement).checked,
            })}
        />
      </label>

      <label class="prefs-tile prefs-tile-metric">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Timeout</span>
          <span class="prefs-tile-meta">Hard stop for one process</span>
        </span>
        <span class="prefs-metric">
          <input
            type="number"
            class="prefs-metric-input prefs-metric-input-wide"
            min="100"
            step="100"
            inputmode="numeric"
            value={workshopDefaults.draft.shellTimeoutMs ?? 30_000}
            disabled={workshopDefaults.saving}
            aria-label="Shell timeout in milliseconds"
            oninput={setTimeoutMs}
          />
          <span class="prefs-metric-unit">ms</span>
        </span>
      </label>

      <label class="prefs-tile prefs-tile-metric">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Max output</span>
          <span class="prefs-tile-meta">Stdout/stderr before truncation</span>
        </span>
        <span class="prefs-metric">
          <input
            type="number"
            class="prefs-metric-input prefs-metric-input-wide"
            min="1024"
            step="1024"
            inputmode="numeric"
            value={workshopDefaults.draft.shellMaxOutputBytes ?? 262_144}
            disabled={workshopDefaults.saving}
            aria-label="Shell max output in bytes"
            oninput={setMaxOutput}
          />
          <span class="prefs-metric-unit">bytes</span>
        </span>
      </label>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">
            Grapheme <span class="font-mono">shell</span>
          </span>
          <span class="prefs-tile-meta">
            {workshop.allowlistEnforce
              ? "Allowlist enforcing for workshop scripts"
              : "Empty allowlist — checking this starts one"}
          </span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={workshop.isModuleAllowed("shell")}
          disabled={workshop.allowlistBusy}
          onchange={(event) =>
            workshop.toggleAllowlistModule(
              "shell",
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
      </label>
    </div>

    {#if agentToolsOn && binariesEmpty}
      <p class="settings-danger-callout mt-3 text-xs leading-relaxed" role="status">
        Agents are unlocked with an empty binary allowlist — list trusted tools below before saving.
      </p>
    {/if}
    {#if workshop.allowlistError}
      <p class="mt-2 text-xs text-content-warning">{workshop.allowlistError}</p>
    {/if}

    <details class="prefs-more" bind:open={allowlistsOpen}>
      <summary class="prefs-more-summary">
        <span class="prefs-more-summary-copy">
          <span>Allowlists</span>
          <span class="prefs-more-summary-meta">
            {binariesEmpty ? "Any basename" : "Custom binaries"}
          </span>
        </span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body space-y-3">
        <label class="block">
          <span class="prefs-tile-title">Allowed binaries</span>
          <span class="prefs-tile-meta mt-0.5 block">One basename per line</span>
          <textarea
            class="rt-mono-input mt-2"
            rows="4"
            value={binariesText}
            oninput={onBinariesInput}
            placeholder={"git\nls\nrg"}
            spellcheck="false"
          ></textarea>
        </label>
        <label class="block">
          <span class="prefs-tile-title">Writable roots</span>
          <span class="prefs-tile-meta mt-0.5 block">Absolute paths</span>
          <textarea
            class="rt-mono-input mt-2"
            rows="3"
            value={writableRootsText}
            oninput={onWritableRootsInput}
            placeholder="/Users/you/projects"
            spellcheck="false"
          ></textarea>
        </label>
      </div>
    </details>
    </div>
  {/if}

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Engine</h3>
      <p class="settings-subsection-lead">Diagnostics and where working memory lives.</p>
    </div>

    <div class="prefs-stack">
      <div class="prefs-grid">
        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Thinking traces</span>
            <span class="prefs-tile-meta">Capture reasoning lines for debugging</span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={workshopDefaults.draft.thinkingCapture ?? true}
            disabled={workshopDefaults.saving}
            onchange={(event) =>
              (workshopDefaults.draft = {
                ...workshopDefaults.draft,
                thinkingCapture: (event.currentTarget as HTMLInputElement).checked,
              })}
          />
        </label>

        <label class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Telemetry export</span>
            <span class="prefs-tile-meta">OpenTelemetry from Stasis</span>
          </span>
          <input
            type="checkbox"
            class="prefs-switch"
            checked={workshopDefaults.draft.stasisOtelEnabled ?? false}
            disabled={workshopDefaults.saving}
            onchange={(event) =>
              (workshopDefaults.draft = {
                ...workshopDefaults.draft,
                stasisOtelEnabled: (event.currentTarget as HTMLInputElement).checked,
              })}
          />
        </label>

        <label class="prefs-tile prefs-tile-metric">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Thinking max lines</span>
            <span class="prefs-tile-meta">Cap retained per turn</span>
          </span>
          <span class="prefs-metric">
            <input
              type="number"
              class="prefs-metric-input prefs-metric-input-wide"
              min="50"
              max="2000"
              inputmode="numeric"
              value={workshopDefaults.draft.thinkingMaxLines ?? 300}
              disabled={workshopDefaults.saving}
              aria-label="Thinking max lines"
              oninput={(event) => numField("thinkingMaxLines", event)}
            />
            <span class="prefs-metric-unit">lines</span>
          </span>
        </label>
      </div>

      {#if nativeWorkloads}
        <div class="rt-active">
        <button
          type="button"
          class="rt-active-trigger"
          class:rt-active-trigger-open={picker === "backend"}
          aria-expanded={picker === "backend"}
          disabled={workshopDefaults.saving}
          onclick={() => togglePicker("backend")}
        >
          <span class="rt-active-copy">
            <span class="rt-active-kicker">Host store</span>
            <span class="rt-active-title">{activeBackendMeta.label}</span>
            <span class="rt-active-meta">{activeBackendMeta.hint}</span>
          </span>
          <span class="rt-active-action workshop-faint">
            {picker === "backend" ? "Close" : "Change"}
          </span>
        </button>
        {#if picker === "backend"}
          <div class="prefs-grid rt-picker" role="listbox" aria-label="Host store">
            {#each BACKEND_OPTIONS as option (option)}
              {@const meta = BACKEND_LABELS[option]}
              <button
                type="button"
                role="option"
                class="prefs-choice"
                class:prefs-choice-active={activeBackend === option}
                aria-selected={activeBackend === option}
                disabled={workshopDefaults.saving}
                onclick={() => setBackend(option)}
              >
                <span class="prefs-choice-label">{meta.label}</span>
                <span class="prefs-choice-hint">{meta.hint}</span>
              </button>
            {/each}
          </div>
        {/if}
        </div>
      {/if}
    </div>

    <details class="prefs-more" bind:open={budgetsOpen}>
      <summary class="prefs-more-summary">
        <span>Tool budgets</span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body">
        <p class="prefs-footnote mb-2">
          Refine helper turns, follow-ups, and restricted mode. Per-turn cap is above.
        </p>
        <div class="prefs-grid">
          {#each toolBudgetFields as field (field.key)}
            <label class="prefs-tile prefs-tile-metric">
              <span class="prefs-tile-copy">
                <span class="prefs-tile-title">{field.label}</span>
                <span class="prefs-tile-meta">{field.hint}</span>
              </span>
              <span class="prefs-metric">
                <input
                  type="number"
                  class="prefs-metric-input"
                  min={field.min}
                  max={field.max}
                  inputmode="numeric"
                  value={workshopDefaults.draft[field.key] ?? ""}
                  disabled={workshopDefaults.saving}
                  aria-label="{field.label} in {field.unit}"
                  oninput={(event) => numField(field.key, event)}
                />
                <span class="prefs-metric-unit">{field.unit}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    </details>

    <details class="prefs-more" bind:open={qualityOpen}>
      <summary class="prefs-more-summary">
        <span>Quality & retries</span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body">
        <div class="prefs-grid">
          {#each [...qualityRetryFields, ...qualityVerifierFields] as field (field.key)}
            <label class="prefs-tile prefs-tile-metric">
              <span class="prefs-tile-copy">
                <span class="prefs-tile-title">{field.label}</span>
                <span class="prefs-tile-meta">{field.hint}</span>
              </span>
              <span class="prefs-metric">
                <input
                  type="number"
                  class="prefs-metric-input {field.wide ? 'prefs-metric-input-wide' : ''}"
                  min={field.min}
                  max={field.max}
                  step={field.step}
                  inputmode="decimal"
                  value={workshopDefaults.draft[field.key] ?? ""}
                  disabled={workshopDefaults.saving}
                  aria-label={field.label}
                  oninput={(event) => numField(field.key, event)}
                />
                <span class="prefs-metric-unit">{field.unit}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    </details>

    <details class="prefs-more" bind:open={hostOpen}>
      <summary class="prefs-more-summary">
        <span>Host env overrides</span>
        <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
      </summary>
      <div class="prefs-more-body">
        <p class="prefs-footnote mb-2">
          <span class="font-mono text-content-secondary">KEY=value</span> per line, applied when a turn
          starts.
        </p>
        <textarea
          class="rt-mono-input"
          rows="4"
          placeholder="KEY=value"
          value={workshopDefaults.draft.envOverrides ?? ""}
          oninput={setEnvOverrides}
        ></textarea>
      </div>
    </details>
  </div>

  {#if nativeWorkloads}
    <SettingsVersionsSection embedded />
    <SettingsWorkerCapacity />
  {/if}

  <div class="rt-save mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar beforeSave={beforeSave} />
  </div>
</section>

<style>
  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: var(--prefs-gap);
  }

  @media (min-width: 720px) {
    .prefs-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .rt-active {
    display: grid;
    gap: var(--prefs-gap);
  }

  .rt-active-trigger {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .rt-active-trigger:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .rt-active-trigger:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .rt-active-trigger-open {
    border-color: rgb(var(--color-primary-500) / 0.35);
    background: rgb(var(--color-primary-500) / 0.08);
  }

  .rt-active-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .rt-active-kicker {
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-quiet));
  }

  .rt-active-title {
    font-size: 0.85rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .rt-active-meta {
    font-size: 0.7rem;
    line-height: 1.35;
    color: rgb(var(--theme-text-quiet));
  }

  .rt-active-action {
    flex-shrink: 0;
    font-size: 0.72rem;
    font-weight: 600;
  }

  .prefs-choice {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .prefs-choice:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .prefs-choice:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-choice-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
    box-shadow: inset 0 0 0 1px rgb(var(--color-primary-500) / 0.18);
  }

  .prefs-choice-label {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-choice-hint {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-tile-metric {
    cursor: default;
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .prefs-metric {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .prefs-metric-input {
    width: 3.4rem;
    border-radius: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-950) / 0.45);
    padding: 0.2rem 0.35rem;
    text-align: right;
    font-size: 0.8rem;
    color: rgb(var(--color-surface-100));
  }

  .prefs-metric-input-wide {
    width: 4.5rem;
  }

  .prefs-metric-input:disabled {
    opacity: 0.5;
  }

  .prefs-metric-unit {
    font-size: 0.68rem;
    color: rgb(var(--theme-text-quiet));
  }

  .prefs-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .prefs-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .prefs-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .prefs-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .prefs-switch:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.7);
    outline-offset: 2px;
  }

  .prefs-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-more {
    margin-top: 0.75rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
    list-style: none;
  }

  .prefs-more-summary::-webkit-details-marker {
    display: none;
  }

  .prefs-more-summary-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-more-summary-meta {
    font-size: 0.68rem;
    font-weight: 500;
    color: rgb(var(--theme-text-quiet));
  }

  :global(.prefs-more-chevron) {
    transition: transform 160ms ease;
  }

  .prefs-more[open] :global(.prefs-more-chevron) {
    transform: rotate(180deg);
  }

  .prefs-more-body {
    padding: 0 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .prefs-footnote {
    margin: 0.45rem 0 0;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-quiet));
  }

  .rt-mono-input {
    display: block;
    width: 100%;
    resize: vertical;
    min-height: 2.75rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.45);
    background: rgb(var(--color-surface-950) / 0.4);
    padding: 0.55rem 0.7rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-100));
  }

  .rt-mono-input::placeholder {
    color: rgb(var(--theme-text-quiet));
  }

  .rt-mono-input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  .rt-mono-input:disabled,
  .rt-mono-input:read-only {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
