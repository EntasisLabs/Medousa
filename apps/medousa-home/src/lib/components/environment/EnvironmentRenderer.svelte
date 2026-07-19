<script lang="ts">
  import type { Snippet } from "svelte";
  import LayoutEditOverlay from "$lib/components/environment/LayoutEditOverlay.svelte";
  import MobileStackLayoutView from "$lib/components/environment/MobileStackLayoutView.svelte";
  import TilingLayoutEditor from "$lib/components/environment/TilingLayoutEditor.svelte";
  import TilingLayoutView from "$lib/components/environment/TilingLayoutView.svelte";
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import ChromeActionRenderer from "$lib/components/environment/ChromeActionRenderer.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { layoutEdit, layoutRootForEditing } from "$lib/stores/layoutEdit.svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import { surfaceUsesDashboardFill } from "$lib/utils/environmentPresentation";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import { componentsInReadingOrder, layoutRootToTiling } from "$lib/utils/layoutTiling";
  import { LayoutGrid, Pencil } from "@lucide/svelte";

  interface Props {
    surfaceId: string;
    builtin?: Snippet;
  }

  let { surfaceId, builtin }: Props = $props();

  const autoEditSurfaces = new Set<string>();

  const isMobile = $derived(layout.isMobile);
  const surface = $derived(environment.surfaceById(surfaceId));
  const isCustom = $derived(surface?.kind === "custom");
  const isDashboard = $derived(surfaceUsesDashboardFill(surface?.layout));
  const headerComponents = $derived(environment.componentsForSurface(surfaceId, "header"));
  const mainComponents = $derived(environment.mainComponentsForSurface(surfaceId));
  const layoutRoot = $derived.by(() => layoutRootForEditing(surfaceId));
  const editingLayout = $derived(layoutEdit.isEditingSurface(surfaceId));
  const componentCount = $derived(mainComponents.length);
  const SurfaceIcon = $derived(surface ? environmentIcon(surface.icon) : LayoutGrid);
  const viewTilingRoot = $derived.by(() => {
    if (!layoutRoot || editingLayout) return null;
    return layoutRootToTiling(
      layoutRoot,
      mainComponents.map((component) => component.id),
    );
  });
  /** Phone: one full-width card per widget, desktop reading order. */
  const mobileStackComponents = $derived(
    componentsInReadingOrder(viewTilingRoot, mainComponents),
  );
  const fabComponents = $derived(environment.componentsForSurface(surfaceId, "fab"));
  const inlineComponents = $derived(environment.componentsForSurface(surfaceId, "inline"));
  const sidebarComponents = $derived(environment.componentsForSurface(surfaceId, "sidebar"));

  $effect(() => {
    // Desktop only — never auto-open the tiling editor on a phone.
    if (isMobile || !isCustom || editingLayout || componentCount > 0 || !chat.sessionId) return;
    if (autoEditSurfaces.has(surfaceId)) return;
    autoEditSurfaces.add(surfaceId);
    layoutEdit.begin(surfaceId);
  });

  $effect(() => {
    if (isMobile && editingLayout) {
      layoutEdit.cancel();
    }
  });

  function configString(config: Record<string, unknown>, key: string): string | null {
    const camel = config[key];
    if (typeof camel === "string" && camel.trim()) return camel.trim();
    const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
    return typeof snake === "string" && snake.trim() ? snake.trim() : null;
  }

  function startArranging() {
    if (isMobile) return;
    layoutEdit.begin(surfaceId);
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
    class:environment-renderer-body-dashboard={isCustom && isDashboard && !isMobile}
    class:environment-renderer-body-mobile-stack={isCustom && isMobile}
    class:environment-renderer-body-mobile-fill={isCustom &&
      isMobile &&
      mobileStackComponents.length === 1}
  >
    {#if isCustom}
      <LayoutEditOverlay
        {surfaceId}
        editing={!isMobile && editingLayout}
        dashboard={isDashboard && !isMobile}
      >
        {#if !editingLayout || isMobile}
          <div class="environment-renderer-canvas-bar">
            {#if surface}
              <div class="environment-renderer-surface-title">
                <ShellSidebarExpandButton label="Show rail" />
                <SurfaceIcon size={16} strokeWidth={1.75} aria-hidden="true" />
                <span>{surface.label}</span>
              </div>
            {/if}
            {#if !isMobile}
              <button
                type="button"
                class="environment-renderer-edit-btn"
                onclick={() => layoutEdit.begin(surfaceId)}
              >
                <Pencil size={14} strokeWidth={2} aria-hidden="true" />
                Edit layout
              </button>
            {/if}
          </div>
        {/if}
        {#if !isMobile && editingLayout && chat.sessionId}
          <TilingLayoutEditor
            {surfaceId}
            components={mainComponents}
            sessionId={chat.sessionId}
            profileId={environment.spec?.profileId}
            feedStateForComponent={(componentId) => environment.feedStateForComponent(componentId)}
            padded={!isDashboard}
          />
        {:else if componentCount === 0}
          <div class="environment-renderer-empty-room">
            <div class="environment-renderer-empty-room-icon" aria-hidden="true">
              <LayoutGrid size={32} strokeWidth={1.5} />
            </div>
            <h2 class="environment-renderer-empty-room-title">This room is empty</h2>
            <p class="environment-renderer-empty-room-copy">
              {#if isMobile}
                Arrange widgets on your computer — they show here as a scrollable stack.
              {:else}
                Add your first widget to make this view yours.
              {/if}
            </p>
            {#if !isMobile}
              <button type="button" class="environment-renderer-empty-room-cta" onclick={startArranging}>
                Start arranging
              </button>
            {/if}
          </div>
        {:else if isMobile && chat.sessionId && mobileStackComponents.length > 0}
          <MobileStackLayoutView
            components={mobileStackComponents}
            sessionId={chat.sessionId}
            profileId={environment.spec?.profileId}
            feedStateForComponent={(componentId) => environment.feedStateForComponent(componentId)}
          />
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

  {#if !isMobile && sidebarComponents.length > 0}
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

  .environment-renderer-body-mobile-stack {
    padding: 0.5rem 0.65rem 0;
    overflow: auto;
    -webkit-overflow-scrolling: touch;
    gap: 0;
  }

  /* Single-widget custom home: fill the shell above the tab bar (true fullscreen). */
  .environment-renderer-body-mobile-fill {
    padding: 0;
    overflow: hidden;
    gap: 0;
  }

  .environment-renderer-body-mobile-fill .environment-renderer-canvas-bar {
    padding: 0.45rem 0.85rem 0.35rem;
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

  .environment-renderer-canvas-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.5rem 0.5rem;
  }

  .environment-renderer-surface-title {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .environment-renderer-surface-title span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .environment-renderer-edit-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    border-radius: 0.5rem;
    padding: 0.4rem 0.7rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-800) 65%, transparent);
    cursor: pointer;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }

  .environment-renderer-edit-btn:hover {
    background: color-mix(in srgb, var(--color-surface-700) 75%, transparent);
    border-color: color-mix(in srgb, var(--color-primary-400) 45%, transparent);
    color: rgb(var(--color-surface-50));
  }

  .environment-renderer-empty-room {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.65rem;
    padding: 2.5rem 1.25rem;
    text-align: center;
  }

  .environment-renderer-empty-room-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 999px;
    color: rgb(var(--color-primary-300));
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .environment-renderer-empty-room-title {
    margin: 0;
    font-size: 1.0625rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .environment-renderer-empty-room-copy {
    margin: 0;
    max-width: 22rem;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-400));
  }

  .environment-renderer-empty-room-cta {
    margin-top: 0.35rem;
    border: 0;
    border-radius: 0.55rem;
    padding: 0.55rem 1rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-950));
    background: rgb(var(--color-primary-400));
    cursor: pointer;
    transition: background 140ms ease;
  }

  .environment-renderer-empty-room-cta:hover {
    background: rgb(var(--color-primary-300));
  }

  .environment-renderer-sidebar {
    width: min(22rem, 100%);
    border-left: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    padding: 0.75rem;
    overflow: auto;
  }
</style>
