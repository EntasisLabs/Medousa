<script lang="ts">
  import type { Snippet } from "svelte";
  import LayoutEditOverlay from "$lib/components/environment/LayoutEditOverlay.svelte";
  import TilingLayoutEditor from "$lib/components/environment/TilingLayoutEditor.svelte";
  import TilingLayoutView from "$lib/components/environment/TilingLayoutView.svelte";
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import ChromeActionRenderer from "$lib/components/environment/ChromeActionRenderer.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layoutEdit, layoutRootForEditing } from "$lib/stores/layoutEdit.svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import { surfaceUsesDashboardFill } from "$lib/utils/environmentPresentation";
  import { layoutRootToTiling } from "$lib/utils/layoutTiling";

  interface Props {
    surfaceId: string;
    builtin?: Snippet;
  }

  let { surfaceId, builtin }: Props = $props();

  const surface = $derived(environment.surfaceById(surfaceId));
  const isCustom = $derived(surface?.kind === "custom");
  const isDashboard = $derived(surfaceUsesDashboardFill(surface?.layout));
  const headerComponents = $derived(environment.componentsForSurface(surfaceId, "header"));
  const mainComponents = $derived(environment.mainComponentsForSurface(surfaceId));
  const layoutRoot = $derived.by(() => layoutRootForEditing(surfaceId));
  const editingLayout = $derived(layoutEdit.isEditingSurface(surfaceId));
  const viewTilingRoot = $derived.by(() => {
    if (!layoutRoot || editingLayout) return null;
    return layoutRootToTiling(
      layoutRoot,
      mainComponents.map((component) => component.id),
    );
  });
  const fabComponents = $derived(environment.componentsForSurface(surfaceId, "fab"));
  const inlineComponents = $derived(environment.componentsForSurface(surfaceId, "inline"));
  const sidebarComponents = $derived(environment.componentsForSurface(surfaceId, "sidebar"));

  function configString(config: Record<string, unknown>, key: string): string | null {
    const camel = config[key];
    if (typeof camel === "string" && camel.trim()) return camel.trim();
    const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
    return typeof snake === "string" && snake.trim() ? snake.trim() : null;
  }
</script>

<div class="environment-renderer h-full min-h-0" data-surface-id={surfaceId}>
  {#if headerComponents.length > 0}
    <div class="environment-renderer-header">
      {#each headerComponents as component (component.id)}
        {#if component.type === "chrome_action"}
          <ChromeActionRenderer {component} variant="header" />
        {/if}
      {/each}
    </div>
  {/if}

  {#if inlineComponents.length > 0}
    <div class="environment-renderer-inline">
      {#each inlineComponents as component (component.id)}
        {#if component.type === "chrome_action"}
          <ChromeActionRenderer {component} variant="inline" />
        {/if}
      {/each}
    </div>
  {/if}

  <div
    class="environment-renderer-body"
    class:environment-renderer-body-custom={isCustom}
    class:environment-renderer-body-dashboard={isCustom && isDashboard}
  >
    {#if isCustom}
      <LayoutEditOverlay {surfaceId} editing={editingLayout}>
        {#if !editingLayout}
          <div class="environment-renderer-edit-entry">
            <button type="button" class="environment-renderer-edit-btn" onclick={() => layoutEdit.begin(surfaceId)}>
              Edit layout
            </button>
          </div>
        {/if}
        {#if editingLayout && chat.sessionId}
          <TilingLayoutEditor
            {surfaceId}
            components={mainComponents}
            sessionId={chat.sessionId}
            profileId={environment.spec?.profileId}
            feedStateForComponent={(componentId) => environment.feedStateForComponent(componentId)}
            padded={!isDashboard}
          />
        {:else if mainComponents.length === 0}
          <p class="environment-renderer-empty">This surface has no components yet. Edit layout to add widgets.</p>
        {:else if viewTilingRoot && chat.sessionId}
          <div
            class="layout-edit-canvas"
            class:layout-edit-canvas-dashboard={isDashboard}
          >
            <TilingLayoutView
              root={viewTilingRoot}
              components={mainComponents}
              sessionId={chat.sessionId}
              profileId={environment.spec?.profileId}
              feedStateForComponent={(componentId) => environment.feedStateForComponent(componentId)}
              editing={false}
              padded={!isDashboard}
            />
          </div>
        {/if}
      </LayoutEditOverlay>
    {:else if builtin}
      {@render builtin()}
    {/if}
  </div>

  {#if sidebarComponents.length > 0}
    <aside class="environment-renderer-sidebar">
      {#each sidebarComponents as component (component.id)}
        {#if component.type === "presentation"}
          {@const artifactId = configString(component.config, "artifactId")}
          {#if artifactId && chat.sessionId}
            <PresentationFrame
              sessionId={chat.sessionId}
              componentId={component.id}
              profileId={environment.spec?.profileId}
              artifactId={artifactId}
              label={component.label ?? "Presentation"}
              mode="panel"
              compact={true}
            />
          {/if}
        {/if}
      {/each}
    </aside>
  {/if}

  {#each fabComponents as component (component.id)}
    {#if component.type === "chrome_action"}
      <ChromeActionRenderer {component} variant="fab" />
    {/if}
  {/each}
</div>

<style>
  .environment-renderer {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    position: relative;
  }

  .environment-renderer-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
  }

  .environment-renderer-inline {
    padding: 0 0.75rem;
  }

  .environment-renderer-body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .environment-renderer-body-custom {
    padding: 0.75rem;
    overflow: auto;
    gap: 0.75rem;
  }

  .environment-renderer-body-dashboard {
    padding: 0;
    overflow: hidden;
    gap: 0;
  }

  .environment-renderer-main-item-fill {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    width: 100%;
  }

  .environment-renderer-body-dashboard :global(.presentation-frame) {
    flex: 1 1 auto;
    min-height: 0;
    border-radius: 0;
    border-left: 0;
    border-right: 0;
    box-shadow: none;
  }

  .layout-edit-canvas {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .layout-edit-canvas-dashboard {
    height: 100%;
  }

  .layout-edit-canvas-dashboard :global(.layout-widget-tile) {
    border-radius: 0;
    box-shadow: none;
  }

  .layout-edit-canvas-dashboard :global(.layout-region-pane-view) {
    border-radius: 0;
  }

  .environment-renderer-sidebar {
    width: min(22rem, 100%);
    border-left: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    padding: 0.75rem;
    overflow: auto;
  }

  .environment-renderer-edit-entry {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
    padding: 0.35rem 0.5rem 0;
  }

  .environment-renderer-edit-btn {
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    border-radius: 0.45rem;
    padding: 0.25rem 0.55rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-200));
    background: transparent;
    cursor: pointer;
  }

  .environment-renderer-empty {
    margin: 0;
    padding: 2rem 1rem;
    text-align: center;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-400));
  }

  .environment-renderer-unsupported {
    margin: 0;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border: 1px dashed color-mix(in srgb, var(--color-surface-600) 70%, transparent);
    font-size: 0.75rem;
    color: rgb(var(--color-surface-400));
  }
</style>
