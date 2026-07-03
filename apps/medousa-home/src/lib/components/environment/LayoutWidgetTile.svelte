<script lang="ts">
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import ChromeActionRenderer from "$lib/components/environment/ChromeActionRenderer.svelte";
  import EnvironmentMedousaView from "$lib/components/environment/EnvironmentMedousaView.svelte";
  import MediaEmbedFrame from "$lib/components/environment/MediaEmbedFrame.svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import { GripVertical, X } from "@lucide/svelte";

  interface Props {
    component: ComponentDef;
    sessionId: string;
    profileId?: string | null;
    feedState?: Record<string, unknown> | null;
    editing?: boolean;
    draggable?: boolean;
    dragging?: boolean;
    onHandlePointerDown?: (event: PointerEvent) => void;
    onRemove?: () => void;
  }

  let {
    component,
    sessionId,
    profileId = null,
    feedState = null,
    editing = false,
    draggable = false,
    dragging = false,
    onHandlePointerDown,
    onRemove,
  }: Props = $props();

  function configString(config: Record<string, unknown>, key: string): string | null {
    const camel = config[key];
    if (typeof camel === "string" && camel.trim()) return camel.trim();
    const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
    return typeof snake === "string" && snake.trim() ? snake.trim() : null;
  }

  function handleHandlePointerDown(event: PointerEvent) {
    if (!draggable || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    onHandlePointerDown?.(event);
  }

  function handleRemove(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    onRemove?.();
  }
</script>

<div
  class="layout-widget-tile"
  class:layout-widget-tile-dragging={dragging}
  class:layout-widget-tile-editing={editing}
>
  {#if draggable}
    <div
      class="layout-widget-tile-handle"
      onpointerdown={handleHandlePointerDown}
    >
      <GripVertical size={14} strokeWidth={2} aria-hidden="true" />
      <span class="layout-widget-tile-handle-label">{component.label ?? "Widget"}</span>
      {#if onRemove}
        <button
          type="button"
          class="layout-widget-tile-remove"
          aria-label={`Remove ${component.label ?? "widget"}`}
          onclick={handleRemove}
        >
          <X size={12} strokeWidth={2.5} aria-hidden="true" />
        </button>
      {/if}
    </div>
  {/if}
  <div class="layout-widget-tile-body">
    {#if component.type === "presentation"}
      {@const artifactId = configString(component.config, "artifactId")}
      {#if artifactId}
        <PresentationFrame
          {sessionId}
          componentId={component.id}
          {profileId}
          artifactId={artifactId}
          label={component.label ?? "Presentation"}
          mode="panel"
          bare={true}
          feedState={feedState ?? null}
          subscribedFeedIds={component.feeds ?? []}
          manageable={false}
        />
      {/if}
    {:else if component.type === "medousa_view"}
      {@const notePath = configString(component.config, "notePath")}
      {#if notePath}
        <EnvironmentMedousaView {notePath} fill={true} />
      {/if}
    {:else if component.type === "media_embed"}
      <MediaEmbedFrame config={component.config} label={component.label} fill={true} />
    {:else if component.type === "chrome_action"}
      <ChromeActionRenderer {component} variant="inline" />
    {/if}
  </div>
</div>

<style>
  .layout-widget-tile {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    height: 100%;
    border-radius: 0.85rem;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-surface-900) 55%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-surface-600) 35%, transparent);
  }

  .layout-widget-tile-dragging {
    opacity: 0.55;
  }

  .layout-widget-tile-handle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 0;
    padding: 0.4rem 0.6rem;
    background: color-mix(in srgb, var(--color-surface-800) 90%, transparent);
    color: rgb(var(--color-surface-200));
    font-size: 0.6875rem;
    font-weight: 500;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }

  .layout-widget-tile-handle-label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .layout-widget-tile-handle:active {
    cursor: grabbing;
  }

  .layout-widget-tile-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: auto;
    width: 1.25rem;
    height: 1.25rem;
    border-radius: 0.35rem;
    color: rgb(var(--color-surface-400));
    background: transparent;
    cursor: pointer;
  }

  .layout-widget-tile-remove:hover {
    color: rgb(var(--color-error-300));
    background: color-mix(in srgb, var(--color-error-500) 18%, transparent);
  }

  .layout-widget-tile-body {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .layout-widget-tile-editing .layout-widget-tile-body {
    pointer-events: none;
  }

  .layout-widget-tile-body :global(.presentation-frame),
  .layout-widget-tile-body :global(.presentation-frame-fill),
  .layout-widget-tile-body :global(.media-embed-frame-fill) {
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
    border-radius: 0;
    border: 0;
    box-shadow: none;
  }
</style>
