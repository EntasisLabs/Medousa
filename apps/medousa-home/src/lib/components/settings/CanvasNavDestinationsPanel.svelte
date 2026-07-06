<script lang="ts">
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { SurfaceDef } from "$lib/types/environment";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import {
    isSurfaceNavVisible,
    NAV_DESTINATION_GROUPS,
  } from "$lib/utils/environmentLayout";
  import { ensurePeersSurfaceInSpec } from "$lib/utils/environmentDefault";

  interface Props {
    spec: NonNullable<typeof environment.spec>;
  }

  let { spec }: Props = $props();

  let busySurfaceId = $state<string | null>(null);
  let error = $state<string | null>(null);

  const liveSpec = $derived(ensurePeersSurfaceInSpec(spec));
  const customSurfaces = $derived(liveSpec.surfaces.filter((surface) => surface.kind === "custom"));
  const surfaceById = $derived(new Map(liveSpec.surfaces.map((surface) => [surface.id, surface])));

  function resolveSurfaces(ids: string[]): SurfaceDef[] {
    return ids
      .map((id) => surfaceById.get(id))
      .filter((surface): surface is SurfaceDef => Boolean(surface));
  }

  function isVisible(surfaceId: string): boolean {
    return isSurfaceNavVisible(liveSpec, surfaceId);
  }

  async function setVisible(surfaceId: string, visible: boolean) {
    if (busySurfaceId) return;
    busySurfaceId = surfaceId;
    error = null;
    try {
      await environment.setSurfaceNavVisible(surfaceId, visible);
      if (
        !visible &&
        !layout.isMobile &&
        layout.desktopSurface === surfaceId
      ) {
        const fallback =
          environment.navSurfaces().find((surface) => surface.id !== surfaceId)?.id ?? "chat";
        layout.navigateDesktop(fallback, { bump: true });
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busySurfaceId = null;
    }
  }
</script>

<div class="canvas-nav-destinations">
  {#each NAV_DESTINATION_GROUPS as group (group.label)}
    {@const surfaces = resolveSurfaces(group.surfaceIds)}
    {#if surfaces.length > 0}
      <div class="canvas-nav-destinations-group">
        <p class="canvas-nav-destinations-group-label">{group.label}</p>
        <div class="settings-toggle-list">
          {#each surfaces as surface (surface.id)}
            {@const Icon = environmentIcon(surface.icon)}
            <label class="settings-toggle-row canvas-nav-destinations-row">
              <span class="canvas-nav-destinations-copy">
                <span class="canvas-nav-destinations-icon" aria-hidden="true">
                  <Icon size={15} strokeWidth={1.75} />
                </span>
                <span class="min-w-0 flex-1">
                  <span class="block text-sm font-medium text-surface-100">{surface.label}</span>
                </span>
              </span>
              <input
                type="checkbox"
                class="checkbox shrink-0"
                checked={isVisible(surface.id)}
                disabled={busySurfaceId === surface.id}
                onchange={(event) =>
                  void setVisible(surface.id, (event.currentTarget as HTMLInputElement).checked)}
              />
            </label>
          {/each}
        </div>
      </div>
    {/if}
  {/each}

  {#if customSurfaces.length > 0}
    <div class="canvas-nav-destinations-group">
      <p class="canvas-nav-destinations-group-label">Your views</p>
      <div class="settings-toggle-list">
        {#each customSurfaces as surface (surface.id)}
          {@const Icon = environmentIcon(surface.icon)}
          <label class="settings-toggle-row canvas-nav-destinations-row">
            <span class="canvas-nav-destinations-copy">
              <span class="canvas-nav-destinations-icon" aria-hidden="true">
                <Icon size={15} strokeWidth={1.75} />
              </span>
              <span class="min-w-0 flex-1">
                <span class="block text-sm font-medium text-surface-100">{surface.label}</span>
              </span>
            </span>
            <input
              type="checkbox"
              class="checkbox shrink-0"
              checked={isVisible(surface.id)}
              disabled={busySurfaceId === surface.id}
              onchange={(event) =>
                void setVisible(surface.id, (event.currentTarget as HTMLInputElement).checked)}
            />
          </label>
        {/each}
      </div>
    </div>
  {/if}

  <p class="canvas-nav-destinations-note workshop-faint">
    Settings and Runtime always stay reachable. Changes apply to the active layout preset ({liveSpec.layoutPresets?.find((preset) => preset.active)?.label ?? "current"}).
  </p>

  {#if error}
    <p class="canvas-nav-destinations-error">{error}</p>
  {/if}
</div>

<style>
  .canvas-nav-destinations {
    display: grid;
    gap: 0.85rem;
  }

  .canvas-nav-destinations-group {
    display: grid;
    gap: 0.35rem;
  }

  .canvas-nav-destinations-group-label {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .canvas-nav-destinations-row {
    gap: 0.75rem;
  }

  .canvas-nav-destinations-copy {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
    flex: 1 1 auto;
  }

  .canvas-nav-destinations-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.65rem;
    height: 1.65rem;
    flex-shrink: 0;
    border-radius: 0.4rem;
    color: rgb(var(--color-surface-300));
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
  }

  .canvas-nav-destinations-note {
    margin: 0;
    font-size: 0.6875rem;
    line-height: 1.45;
  }

  .canvas-nav-destinations-error {
    margin: 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
