<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import CanvasIconPicker from "$lib/components/settings/CanvasIconPicker.svelte";
  import { slugifyCanvasId } from "$lib/utils/environmentCanvasOps";
  import type { SurfaceLayout } from "$lib/types/environment";
  import type { AllowedSurfaceIcon } from "$lib/utils/environmentIconCatalog";

  interface Props {
    onCreated?: (surfaceId: string) => void;
  }

  let { onCreated }: Props = $props();

  let open = $state(false);
  let label = $state("");
  let surfaceId = $state("");
  let idTouched = $state(false);
  let icon = $state<AllowedSurfaceIcon>("sparkles");
  let layout = $state<SurfaceLayout>("dashboard");
  let afterSurfaceId = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const presets = $derived(environment.spec?.layoutPresets ?? []);
  const activePreset = $derived(
    presets.find((preset) => preset.active) ??
      presets.find((preset) => preset.id === environment.spec?.activePresetId) ??
      null,
  );
  const navOptions = $derived.by(() => {
    const spec = environment.spec;
    if (!spec || !activePreset) return [];
    const byId = new Map(spec.surfaces.map((surface) => [surface.id, surface]));
    return activePreset.surfaces
      .map((id) => byId.get(id))
      .filter((surface): surface is NonNullable<typeof surface> => Boolean(surface));
  });

  const previewId = $derived(
    idTouched ? slugifyCanvasId(surfaceId) : slugifyCanvasId(label || surfaceId),
  );

  $effect(() => {
    if (!idTouched && label) {
      surfaceId = slugifyCanvasId(label);
    }
  });

  async function submit() {
    error = null;
    busy = true;
    try {
      const createdId = await environment.addCustomView({
        id: previewId,
        label: label.trim(),
        icon,
        layout,
        presetId: activePreset?.id ?? null,
        afterSurfaceId: afterSurfaceId || null,
      });
      label = "";
      surfaceId = "";
      idTouched = false;
      icon = "sparkles";
      layout = "dashboard";
      afterSurfaceId = "";
      open = false;
      onCreated?.(createdId);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="canvas-add-view">
  <button type="button" class="canvas-add-view-toggle" onclick={() => (open = !open)}>
    {open ? "Cancel" : "+ New view"}
  </button>

  {#if open}
    <form
      class="canvas-add-view-form"
      onsubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label class="canvas-field">
        <span>Name</span>
        <input
          type="text"
          bind:value={label}
          placeholder="Writing studio"
          required
          disabled={busy}
        />
      </label>

      <label class="canvas-field">
        <span>View id</span>
        <input
          type="text"
          bind:value={surfaceId}
          oninput={() => {
            idTouched = true;
          }}
          placeholder={previewId || "writing-studio"}
          disabled={busy}
        />
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

      <label class="canvas-field">
        <span>Layout</span>
        <select bind:value={layout} disabled={busy}>
          <option value="dashboard">Dashboard (fill screen)</option>
          <option value="single">Single column</option>
          <option value="split">Split</option>
        </select>
      </label>

      <label class="canvas-field">
        <span>Nav position ({activePreset?.label ?? "active preset"})</span>
        <select bind:value={afterSurfaceId} disabled={busy}>
          <option value="">End of nav</option>
          {#each navOptions as surface (surface.id)}
            <option value={surface.id}>After {surface.label}</option>
          {/each}
        </select>
      </label>

      {#if error}
        <p class="canvas-form-error">{error}</p>
      {/if}

      <button type="submit" class="btn btn-sm btn-primary" disabled={busy || !label.trim()}>
        {busy ? "Creating…" : "Create view"}
      </button>
    </form>
  {/if}
</div>

<style>
  .canvas-add-view {
    margin-top: 0.75rem;
  }

  .canvas-add-view-toggle {
    border: 0;
    padding: 0.15rem 0;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--color-primary-400));
    background: transparent;
    cursor: pointer;
  }

  .canvas-add-view-toggle:hover {
    color: rgb(var(--color-primary-300));
  }

  html:not(.dark) .canvas-add-view-toggle {
    color: rgb(var(--color-primary-600));
  }

  html:not(.dark) .canvas-add-view-toggle:hover {
    color: rgb(var(--color-primary-700));
  }

  .canvas-add-view-form {
    display: grid;
    gap: 0.65rem;
    margin-top: 0.75rem;
    padding: 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-600)) / 0.45);
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-900)) / 0.35);
  }

  .canvas-field {
    display: grid;
    gap: 0.25rem;
    font-size: 0.75rem;
  }

  .canvas-field span {
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .canvas-field input,
  .canvas-field select {
    border-radius: 0.45rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-600)) / 0.55);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.6);
    padding: 0.35rem 0.5rem;
    font-size: 0.8125rem;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .canvas-form-error {
    margin: 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
