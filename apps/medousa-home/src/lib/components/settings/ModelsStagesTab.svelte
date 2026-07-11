<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";

  interface Props {
    disabled?: boolean;
    mobile?: boolean;
  }

  let { disabled = false, mobile = false }: Props = $props();

  const STAGE_ROLE_LABELS: Record<string, string> = {
    orchestrator: "Lead",
    chunker: "Reader",
    extractor: "Extractor",
    summarizer: "Summarizer",
    verifier: "Verifier",
    packer: "Packer",
    final_response: "Final voice",
  };
</script>

<div class="models-stages">
  <p class="settings-subsection-lead">
    Which model handles each pipeline stage. Stance and answer depth live in Voice; live routes show
    in Runtime → Routing.
  </p>

  <div class="settings-toggle-list">
    <label class="settings-toggle-row settings-metric-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Stage</span>
        <span class="workshop-faint mt-0.5 block text-xs">Pick a role to edit its model</span>
      </span>
      <select
        class="models-stages-select"
        bind:value={workshopDefaults.selectedRouteRole}
        disabled={disabled || workshopDefaults.routeRoles().length === 0}
      >
        {#each workshopDefaults.routeRoles() as role (role)}
          <option value={role}>{STAGE_ROLE_LABELS[role] ?? role}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if workshopDefaults.selectedRoute()}
    {@const route = workshopDefaults.selectedRoute()!}
    <div class="models-stages-grid mt-3">
      <label class="models-stages-field">
        <span>Provider</span>
        <input
          class="models-stages-input"
          value={route.provider}
          readonly={disabled}
          disabled={disabled}
          oninput={(e) =>
            workshopDefaults.updateSelectedRoute({
              provider: (e.currentTarget as HTMLInputElement).value,
            })}
        />
      </label>
      <label class="models-stages-field">
        <span>Model</span>
        <input
          class="models-stages-input"
          value={route.model}
          readonly={disabled}
          disabled={disabled}
          oninput={(e) =>
            workshopDefaults.updateSelectedRoute({
              model: (e.currentTarget as HTMLInputElement).value,
            })}
        />
      </label>
      <label class="models-stages-field models-stages-field-wide">
        <span>Fallback chain</span>
        <input
          class="models-stages-input font-mono text-xs"
          value={route.fallback_chain.join(", ")}
          readonly={disabled}
          disabled={disabled}
          placeholder="provider:model, …"
          oninput={(e) =>
            workshopDefaults.updateSelectedRoute({
              fallback_chain: (e.currentTarget as HTMLInputElement).value
                .split(",")
                .map((entry) => entry.trim())
                .filter(Boolean),
            })}
        />
      </label>
    </div>
  {/if}

  <div class="mt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</div>

<style>
  .models-stages-grid {
    display: grid;
    gap: 0.65rem;
    grid-template-columns: 1fr;
  }

  @media (min-width: 640px) {
    .models-stages-grid {
      grid-template-columns: 1fr 1fr;
    }
  }

  .models-stages-field {
    display: grid;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .models-stages-field-wide {
    grid-column: 1 / -1;
  }

  .models-stages-input,
  .models-stages-select {
    border-radius: 0.45rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.55);
    padding: 0.4rem 0.55rem;
    font-size: 0.8125rem;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .models-stages-select {
    max-width: 11rem;
    flex-shrink: 0;
  }

  .models-stages-input:focus,
  .models-stages-select:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  .models-stages-input:disabled,
  .models-stages-select:disabled,
  .models-stages-input:read-only {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
