<script lang="ts">
  import EnvironmentPresetSwitcher from "$lib/components/environment/EnvironmentPresetSwitcher.svelte";
  import { environment } from "$lib/stores/environment.svelte";

  const spec = $derived(environment.spec);
  const pending = $derived(environment.pendingProposal);
  const customSurfaces = $derived(
    (spec?.surfaces ?? []).filter((surface) => surface.kind === "custom"),
  );
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Canvas</h2>
    <p class="workshop-faint mt-1 text-sm">
      Layout presets, custom surfaces, and agent-proposed environment changes.
    </p>
  </header>

  <div class="mt-4">
    <EnvironmentPresetSwitcher />
  </div>

  {#if spec}
    <dl class="settings-kv mt-6">
      <div>
        <dt>Active preset</dt>
        <dd>{spec.activePresetId ?? "default"}</dd>
      </div>
      <div>
        <dt>Surfaces</dt>
        <dd>{spec.surfaces.length}</dd>
      </div>
      <div>
        <dt>Components</dt>
        <dd>{spec.components.length}</dd>
      </div>
      <div>
        <dt>Custom surfaces</dt>
        <dd>{customSurfaces.map((s) => s.id).join(", ") || "none"}</dd>
      </div>
    </dl>
  {/if}

  {#if pending}
    <div class="env-pending-card mt-6">
      <p class="text-sm font-medium text-surface-100">Pending workshop layout</p>
      <p class="workshop-faint mt-1 text-xs">{pending.diffSummary}</p>
      <p class="workshop-faint mt-1 text-xs">Proposed by {pending.proposedBy}</p>
      {#if pending.errors.length > 0}
        <ul class="env-pending-errors mt-2 text-xs text-error-300">
          {#each pending.errors as error (error)}
            <li>{error}</li>
          {/each}
        </ul>
      {/if}
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={environment.pendingBusy || pending.errors.length > 0}
          onclick={() => void environment.applyPendingProposal()}
        >
          Apply layout
        </button>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          disabled={environment.pendingBusy}
          onclick={() => void environment.dismissPendingProposal()}
        >
          Dismiss
        </button>
      </div>
    </div>
  {/if}
</section>

<style>
  .settings-kv {
    display: grid;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }

  .settings-kv dt {
    color: rgb(var(--color-surface-500));
  }

  .settings-kv dd {
    margin: 0;
    color: rgb(var(--color-surface-100));
  }

  .env-pending-card {
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
    padding: 0.875rem 1rem;
  }

  .env-pending-errors {
    margin: 0;
    padding-left: 1rem;
  }
</style>
