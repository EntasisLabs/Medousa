<script lang="ts">
  import type { Snippet } from "svelte";
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import ChromeActionRenderer from "$lib/components/environment/ChromeActionRenderer.svelte";
  import EnvironmentMedousaView from "$lib/components/environment/EnvironmentMedousaView.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import type { ComponentDef } from "$lib/types/environment";
  import type { ArtifactEmbedMode } from "$lib/utils/artifactPrepareHtml";

  interface Props {
    surfaceId: string;
    builtin?: Snippet;
  }

  let { surfaceId, builtin }: Props = $props();

  const surface = $derived(environment.surfaceById(surfaceId));
  const isCustom = $derived(surface?.kind === "custom");
  const headerComponents = $derived(environment.componentsForSurface(surfaceId, "header"));
  const mainComponents = $derived(environment.componentsForSurface(surfaceId, "main"));
  const fabComponents = $derived(environment.componentsForSurface(surfaceId, "fab"));
  const inlineComponents = $derived(environment.componentsForSurface(surfaceId, "inline"));
  const sidebarComponents = $derived(environment.componentsForSurface(surfaceId, "sidebar"));

  function presentationMode(component: ComponentDef): ArtifactEmbedMode {
    if (component.presentation === "panel") return "panel";
    if (component.presentation === "fullscreen") return "fullscreen";
    return "inline";
  }

  function configString(config: Record<string, unknown>, key: string): string | null {
    const camel = config[key];
    if (typeof camel === "string" && camel.trim()) return camel.trim();
    const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
    return typeof snake === "string" && snake.trim() ? snake.trim() : null;
  }
</script>

<div class="environment-renderer" data-surface-id={surfaceId}>
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

  <div class="environment-renderer-body" class:environment-renderer-body-custom={isCustom}>
    {#if isCustom}
      {#if mainComponents.length === 0}
        <p class="environment-renderer-empty">This surface has no components yet.</p>
      {:else}
        {#each mainComponents as component (component.id)}
          {#if component.type === "presentation"}
            {@const artifactId = configString(component.config, "artifactId")}
            {#if artifactId && chat.sessionId}
              <PresentationFrame
                sessionId={chat.sessionId}
                artifactId={artifactId}
                label={component.label ?? "Presentation"}
                mode={presentationMode(component)}
                bare={presentationMode(component) !== "inline"}
              />
            {/if}
          {:else if component.type === "medousa_view"}
            {@const notePath = configString(component.config, "notePath")}
            {#if notePath}
              <EnvironmentMedousaView {notePath} />
            {/if}
          {:else if component.type === "chrome_action"}
            <ChromeActionRenderer {component} variant="inline" />
          {:else}
            <p class="environment-renderer-unsupported">
              Component <code>{component.id}</code> ({component.type}) is not supported in Home
              yet — use <code>presentation</code> or <code>chrome_action</code> on custom surfaces.
            </p>
          {/if}
        {/each}
      {/if}
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

  .environment-renderer-sidebar {
    width: min(22rem, 100%);
    border-left: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    padding: 0.75rem;
    overflow: auto;
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
