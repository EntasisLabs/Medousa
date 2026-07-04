<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import CanvasIconPicker from "$lib/components/settings/CanvasIconPicker.svelte";
  import type { SurfaceDef } from "$lib/types/environment";
  import type { AllowedSurfaceIcon } from "$lib/utils/environmentIconCatalog";
  import { isAllowedSurfaceIcon } from "$lib/utils/environmentIconCatalog";

  interface Props {
    surface: SurfaceDef;
    onSaved?: () => void;
    onCancel?: () => void;
  }

  let { surface, onSaved, onCancel }: Props = $props();

  let label = $state(surface.label);
  let icon = $state<AllowedSurfaceIcon>(
    isAllowedSurfaceIcon(surface.icon) ? surface.icon : "sparkles",
  );
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    label = surface.label;
    icon = isAllowedSurfaceIcon(surface.icon) ? surface.icon : "sparkles";
  });

  async function submit() {
    error = null;
    busy = true;
    try {
      await environment.updateCustomView({
        surfaceId: surface.id,
        label: label.trim(),
        icon,
      });
      onSaved?.();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="canvas-edit-view-form"
  onsubmit={(event) => {
    event.preventDefault();
    void submit();
  }}
>
  <label class="canvas-field">
    <span>Name</span>
    <input type="text" bind:value={label} required disabled={busy} />
  </label>

  <div class="canvas-field">
    <span>Nav icon</span>
    <CanvasIconPicker
      {icon}
      disabled={busy}
      onChange={(next) => {
        icon = next;
      }}
    />
  </div>

  {#if error}
    <p class="canvas-form-error">{error}</p>
  {/if}

  <div class="canvas-edit-view-actions">
    <button type="submit" class="btn btn-sm btn-primary" disabled={busy || !label.trim()}>
      {busy ? "Saving…" : "Save"}
    </button>
    <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={() => onCancel?.()}>
      Cancel
    </button>
  </div>
</form>

<style>
  .canvas-edit-view-form {
    display: grid;
    gap: 0.65rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
    padding: 0.65rem 0.7rem 0.75rem;
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

  .canvas-edit-view-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }
</style>
