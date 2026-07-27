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
  const canDeleteActive = $derived(
    Boolean(activePreset && !isBuiltinLayoutPreset(activePreset.id)),
  );

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

  async function removeActive() {
    if (!activePreset || !canDeleteActive) return;
    error = null;
    deleteBusy = true;
    try {
      await environment.removeLayoutPreset(activePreset.id);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      deleteBusy = false;
    }
  }
</script>

<div class="canvas-add-layout">
  <div class="canvas-add-layout-bar">
    <button type="button" class="canvas-add-layout-toggle" onclick={() => (open = !open)}>
      {open ? "Cancel" : "+ New"}
    </button>
    {#if canDeleteActive}
      <button
        type="button"
        class="canvas-add-layout-delete"
        title="Delete this layout"
        aria-label="Delete {activePreset?.label ?? "layout"}"
        disabled={busy || deleteBusy}
        onclick={() => void removeActive()}
      >
        <Trash2 size={13} strokeWidth={2} aria-hidden="true" />
      </button>
    {/if}
  </div>

  {#if open}
    <form
      class="canvas-add-layout-form"
      onsubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <p class="canvas-add-layout-lead">
        Saves the current rail destinations as a new layout from
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
  {:else if error}
    <p class="canvas-form-error">{error}</p>
  {/if}
</div>

<style>
  .canvas-add-layout {
    margin-top: 0;
  }

  .canvas-add-layout-bar {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .canvas-add-layout-toggle {
    border: 0;
    padding: 0.15rem 0;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-primary-400));
    background: transparent;
    cursor: pointer;
  }

  .canvas-add-layout-toggle:hover {
    color: rgb(var(--color-primary-300));
  }

  .canvas-add-layout-delete {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.3rem;
    padding: 0.2rem;
    color: rgb(var(--color-surface-500));
    background: transparent;
    cursor: pointer;
  }

  .canvas-add-layout-delete:hover:not(:disabled) {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, var(--color-error-600) 10%, transparent);
  }

  .canvas-add-layout-delete:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .canvas-add-layout-form {
    display: grid;
    gap: 0.5rem;
    margin-top: 0.55rem;
    padding: 0.65rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
  }

  .canvas-add-layout-lead {
    margin: 0;
    font-size: 0.6875rem;
    line-height: 1.35;
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
    margin: 0.35rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
