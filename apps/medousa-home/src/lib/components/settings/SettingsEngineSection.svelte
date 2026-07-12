<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { BACKEND_OPTIONS } from "$lib/types/workshopDefaults";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const toolBudgetFields = [
    {
      key: "hostBusMaxToolRounds" as const,
      label: "Specialist rounds",
      hint: "Tool calls allowed when specialists handle a turn",
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
      hint: "Cap when the turn looks light",
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
      hint: "How many times to nudge a text-only stuck turn",
      unit: "tries",
      min: 0,
      max: 12,
    },
    {
      key: "classifierRestrictedMaxToolRounds" as const,
      label: "Restricted budget",
      hint: "Tool calls when the classifier tightens reach",
      unit: "rounds",
      min: 0,
      max: 24,
    },
  ];

  const qualityRetryFields = [
    {
      key: "retryRuntimeMaxRetries" as const,
      label: "Max retries",
      hint: "How many times the runtime may retry a failed step",
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
      hint: "Minimum average support strength across claims",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
    {
      key: "verifierMinSupportedClaimRatio" as const,
      label: "Supported claims",
      hint: "Minimum ratio of claims that pass support checks",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
    {
      key: "verifierMinClaimSupportStrength" as const,
      label: "Claim support",
      hint: "Minimum support strength for an individual claim",
      unit: "0–1",
      min: 0,
      max: 1,
      step: 0.05,
      wide: true,
    },
  ];

  function numField(key: keyof typeof workshopDefaults.draft, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }

  function boolField(key: "thinkingCapture" | "stasisOtelEnabled", event: Event) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: (event.currentTarget as HTMLInputElement).checked,
    };
  }

  function setBackend(value: string) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      backend: value,
    };
  }

  function setEnvOverrides(event: Event) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      envOverrides: (event.currentTarget as HTMLTextAreaElement).value,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Engine</h2>
    <p class="workshop-faint mt-1 text-sm">
      Tool budgets, quality gates, diagnostics, and host plumbing. Stage models and reasoning
      effort live in Models; answer depth lives in Voice.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="settings-subsection-heading">Tool budgets</h3>
    <p class="settings-subsection-lead">
      Per-turn cap lives in Reach. These tighten budgets for specialists, follow-ups, and restricted turns.
    </p>
    <div class="settings-toggle-list">
      {#each toolBudgetFields as field (field.key)}
        <label class="settings-toggle-row settings-metric-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">{field.label}</span>
            <span class="workshop-faint mt-0.5 block text-xs">{field.hint}</span>
          </span>
          <span class="settings-metric-value">
            <input
              type="number"
              class="settings-metric-input"
              min={field.min}
              max={field.max}
              inputmode="numeric"
              value={workshopDefaults.draft[field.key] ?? ""}
              readonly={readOnly}
              disabled={readOnly}
              aria-label="{field.label} in {field.unit}"
              oninput={(event) => numField(field.key, event)}
            />
            <span class="settings-metric-unit" aria-hidden="true">{field.unit}</span>
          </span>
        </label>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Quality</h3>
    <p class="settings-subsection-lead">Retry limits and verifier thresholds for careful answers.</p>
    <div class="settings-toggle-list">
      {#each [...qualityRetryFields, ...qualityVerifierFields] as field (field.key)}
        <label class="settings-toggle-row settings-metric-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">{field.label}</span>
            <span class="workshop-faint mt-0.5 block text-xs">{field.hint}</span>
          </span>
          <span class="settings-metric-value">
            <input
              type="number"
              class="settings-metric-input {field.wide ? 'settings-metric-input-wide' : ''}"
              min={field.min}
              max={field.max}
              step={field.step}
              inputmode="decimal"
              value={workshopDefaults.draft[field.key] ?? ""}
              readonly={readOnly}
              disabled={readOnly}
              aria-label="{field.label}"
              oninput={(event) => numField(field.key, event)}
            />
            <span class="settings-metric-unit" aria-hidden="true">{field.unit}</span>
          </span>
        </label>
      {/each}
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Diagnostics</h3>
    <p class="settings-subsection-lead">What the engine captures while turns run.</p>
    <div class="settings-toggle-list">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Thinking traces</span>
          <span class="workshop-faint mt-0.5 block text-xs">Capture internal reasoning lines for debugging</span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={workshopDefaults.draft.thinkingCapture ?? true}
          disabled={readOnly}
          onchange={(event) => boolField("thinkingCapture", event)}
        />
      </label>
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Stasis OTEL</span>
          <span class="workshop-faint mt-0.5 block text-xs">Export OpenTelemetry from the Stasis runtime</span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={workshopDefaults.draft.stasisOtelEnabled ?? false}
          disabled={readOnly}
          onchange={(event) => boolField("stasisOtelEnabled", event)}
        />
      </label>
      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Thinking max lines</span>
          <span class="workshop-faint mt-0.5 block text-xs">Cap how much thinking is retained per turn</span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input settings-metric-input-wide"
            min="50"
            max="2000"
            inputmode="numeric"
            value={workshopDefaults.draft.thinkingMaxLines ?? 300}
            readonly={readOnly}
            disabled={readOnly}
            aria-label="Thinking max lines"
            oninput={(event) => numField("thinkingMaxLines", event)}
          />
          <span class="settings-metric-unit" aria-hidden="true">lines</span>
        </span>
      </label>
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Host</h3>
    <p class="settings-subsection-lead">
      Storage backend and environment overrides for the workshop daemon.
    </p>

    <div class="mt-1 grid gap-2 sm:grid-cols-3">
      {#each BACKEND_OPTIONS as option (option)}
        <button
          type="button"
          class="settings-depth-card {(workshopDefaults.draft.backend ?? 'surreal-mem') === option
            ? 'settings-depth-card-active'
            : ''}"
          disabled={readOnly}
          aria-pressed={(workshopDefaults.draft.backend ?? "surreal-mem") === option}
          onclick={() => setBackend(option)}
        >
          <span class="block text-sm font-medium text-surface-100">{option}</span>
        </button>
      {/each}
    </div>

    <label class="mt-3 block">
      <span class="settings-subsection-heading mb-0">Env overrides</span>
      <span class="settings-subsection-lead">KEY=value per line — applied when the daemon starts a turn.</span>
      <textarea
        class="engine-env-input"
        rows="4"
        placeholder="KEY=value"
        value={workshopDefaults.draft.envOverrides ?? ""}
        readonly={readOnly}
        disabled={readOnly}
        oninput={setEnvOverrides}
      ></textarea>
    </label>
  </div>

  <div class="mt-6">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>

<style>
  .engine-env-input {
    display: block;
    width: 100%;
    margin-top: 0.35rem;
    resize: vertical;
    min-height: 5rem;
    border-radius: 0.45rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.55);
    padding: 0.55rem 0.7rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .engine-env-input::placeholder {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .engine-env-input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  .engine-env-input:disabled,
  .engine-env-input:read-only {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
