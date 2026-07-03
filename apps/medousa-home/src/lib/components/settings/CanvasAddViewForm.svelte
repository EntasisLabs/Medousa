<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    ALLOWED_SURFACE_ICONS,
    SURFACE_ICON_GROUPS,
    type AllowedSurfaceIcon,
  } from "$lib/utils/environmentIconCatalog";
  import { slugifyCanvasId } from "$lib/utils/environmentCanvasOps";
  import type { SurfaceLayout } from "$lib/types/environment";
  import { ChevronDown } from "@lucide/svelte";

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
  let iconPickerOpen = $state(false);

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

  const SelectedIcon = $derived(environmentIcon(icon));

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
    {open ? "Cancel new view" : "Add custom view"}
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
        <div class="canvas-icon-picker">
          <button
            type="button"
            class="canvas-icon-picker-btn"
            aria-expanded={iconPickerOpen}
            disabled={busy}
            onclick={() => (iconPickerOpen = !iconPickerOpen)}
          >
            <SelectedIcon size={16} strokeWidth={1.75} />
            <span>{icon}</span>
            <ChevronDown size={14} />
          </button>
          {#if iconPickerOpen}
            <div class="canvas-icon-grid" role="listbox">
              {#each Object.entries(SURFACE_ICON_GROUPS) as [group, icons] (group)}
                <p class="canvas-icon-group-label">{group}</p>
                {#each icons as name (name)}
                  {@const Icon = environmentIcon(name)}
                  <button
                    type="button"
                    role="option"
                    aria-selected={icon === name}
                    class="canvas-icon-option"
                    class:canvas-icon-option-active={icon === name}
                    title={name}
                    onclick={() => {
                      icon = name;
                      iconPickerOpen = false;
                    }}
                  >
                    <Icon size={16} strokeWidth={1.75} />
                  </button>
                {/each}
              {/each}
              <p class="canvas-icon-group-label">all</p>
              {#each ALLOWED_SURFACE_ICONS as name (name)}
                {@const Icon = environmentIcon(name)}
                <button
                  type="button"
                  role="option"
                  aria-selected={icon === name}
                  class="canvas-icon-option"
                  class:canvas-icon-option-active={icon === name}
                  title={name}
                  onclick={() => {
                    icon = name;
                    iconPickerOpen = false;
                  }}
                >
                  <Icon size={16} strokeWidth={1.75} />
                </button>
              {/each}
            </div>
          {/if}
        </div>
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
    margin-top: 1rem;
  }

  .canvas-add-view-toggle {
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    border-radius: 0.5rem;
    padding: 0.4rem 0.65rem;
    font-size: 0.75rem;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 10%, transparent);
    cursor: pointer;
  }

  .canvas-add-view-form {
    display: grid;
    gap: 0.65rem;
    margin-top: 0.75rem;
    padding: 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
  }

  .canvas-field {
    display: grid;
    gap: 0.25rem;
    font-size: 0.75rem;
  }

  .canvas-field span {
    color: rgb(var(--color-surface-400));
  }

  .canvas-field input,
  .canvas-field select {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
  }

  .canvas-icon-picker {
    position: relative;
  }

  .canvas-icon-picker-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    cursor: pointer;
  }

  .canvas-icon-grid {
    position: absolute;
    z-index: 20;
    left: 0;
    right: 0;
    top: calc(100% + 0.25rem);
    max-height: 14rem;
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(2rem, 1fr));
    gap: 0.25rem;
    padding: 0.5rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 60%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  .canvas-icon-group-label {
    grid-column: 1 / -1;
    margin: 0.25rem 0 0;
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-surface-500));
  }

  .canvas-icon-option {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border-radius: 0.375rem;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .canvas-icon-option:hover {
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .canvas-icon-option-active {
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 18%, transparent);
  }

  .canvas-form-error {
    margin: 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
