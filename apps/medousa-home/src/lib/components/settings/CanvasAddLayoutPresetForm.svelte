<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { presetDisplayLabel } from "$lib/utils/customViewStatus";
  import { isBuiltinLayoutPreset } from "$lib/utils/environmentLayout";
  import { Trash2 } from "@lucide/svelte";

  let open = $state(false);
  let label = $state("");
  let busy = $state(false);
  let deleteBusy = $state(false);
  let error = $state<string | null>(null);

  const presets = $derived(environment.spec?.layoutPresets ?? []);
  const activePreset = $derived(
    presets.find((preset) => preset.active) ??
      presets.find((preset) => preset.id === environment.spec?.activePresetId) ??
      null,
  );
  const customPresets = $derived(presets.filter((preset) => !isBuiltinLayoutPreset(preset.id)));

  async function submit() {
    error = null;
    busy = true;
    try {
      await environment.addLayoutPreset({ label: label.trim() });
      label = "";
      open = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function removePreset(presetId: string) {
    error = null;
    deleteBusy = true;
    try {
      await environment.removeLayoutPreset(presetId);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      deleteBusy = false;
    }
  }
</script>

<div class="canvas-add-layout">
  <button type="button" class="canvas-add-layout-toggle" onclick={() => (open = !open)}>
    {open ? "Cancel" : "+ New layout"}
  </button>

  {#if open}
    <form
      class="canvas-add-layout-form"
      onsubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <p class="canvas-add-layout-lead">
        Saves the current nav destinations as a new layout and switches to it. Start from
        {presetDisplayLabel(activePreset?.id ?? "default", activePreset?.label)}.
      </p>

      <label class="canvas-field">
        <span>Layout name</span>
        <input
          type="text"
          bind:value={label}
          placeholder="Writing mode"
          required
          disabled={busy || deleteBusy}
        />
      </label>

      {#if error}
        <p class="canvas-form-error">{error}</p>
      {/if}

      <button
        type="submit"
        class="btn btn-sm btn-primary"
        disabled={busy || deleteBusy || !label.trim()}
      >
        {busy ? "Creating…" : "Create layout"}
      </button>
    </form>
  {/if}

  {#if customPresets.length > 0}
    <ul class="canvas-custom-layout-list">
      {#each customPresets as preset (preset.id)}
        {@const isActive = preset.id === activePreset?.id}
        <li class="canvas-custom-layout-row">
          <span class="canvas-custom-layout-label">
            {presetDisplayLabel(preset.id, preset.label)}
            {#if isActive}
              <span class="canvas-custom-layout-active">Active</span>
            {/if}
          </span>
          {#if !isActive}
            <button
              type="button"
              class="canvas-custom-layout-delete"
              title="Delete layout"
              aria-label="Delete {preset.label}"
              disabled={busy || deleteBusy}
              onclick={() => void removePreset(preset.id)}
            >
              <Trash2 size={14} strokeWidth={2} />
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .canvas-add-layout {
    margin-top: 0.75rem;
  }

  .canvas-add-layout-toggle {
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    border-radius: 0.5rem;
    padding: 0.4rem 0.65rem;
    font-size: 0.75rem;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 10%, transparent);
    cursor: pointer;
  }

  .canvas-add-layout-form {
    display: grid;
    gap: 0.65rem;
    margin-top: 0.75rem;
    padding: 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
  }

  .canvas-add-layout-lead {
    margin: 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .canvas-field {
    display: grid;
    gap: 0.25rem;
    font-size: 0.75rem;
  }

  .canvas-field span {
    color: rgb(var(--color-surface-400));
  }

  .canvas-field input {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
  }

  .canvas-form-error {
    margin: 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .canvas-custom-layout-list {
    list-style: none;
    margin: 0.65rem 0 0;
    padding: 0;
    display: grid;
    gap: 0.35rem;
  }

  .canvas-custom-layout-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.35rem 0.45rem;
    border-radius: 0.45rem;
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
  }

  .canvas-custom-layout-label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
  }

  .canvas-custom-layout-active {
    font-size: 0.625rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-primary-300));
  }

  .canvas-custom-layout-delete {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.35rem;
    padding: 0.25rem;
    color: rgb(var(--color-surface-500));
    background: transparent;
    cursor: pointer;
  }

  .canvas-custom-layout-delete:hover:not(:disabled) {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, var(--color-error-600) 10%, transparent);
  }

  .canvas-custom-layout-delete:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
