<script lang="ts">
  import Self from "$lib/components/environment/LayoutNodeRenderer.svelte";
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import ChromeActionRenderer from "$lib/components/environment/ChromeActionRenderer.svelte";
  import EnvironmentMedousaView from "$lib/components/environment/EnvironmentMedousaView.svelte";
  import type { ComponentDef, LayoutNode, SurfaceLayout } from "$lib/types/environment";
  import {
    presentationBare,
    presentationEmbedMode,
    surfaceUsesDashboardFill,
  } from "$lib/utils/environmentPresentation";
  import {
    componentById,
    distributionToJustify,
    flexStyle,
    spacingToGap,
    stackCrossAlign,
    type LayoutFillContext,
  } from "$lib/utils/layoutPresentation";

  interface Props {
    node: LayoutNode;
    surfaceId: string;
    surfaceLayout: SurfaceLayout | undefined;
    components: ComponentDef[];
    sessionId: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
    parentType?: "vstack" | "hstack" | "grid" | null;
    siblingCount?: number;
    distribution?: import("$lib/types/environment").StackDistribution;
  }

  let {
    node,
    surfaceId,
    surfaceLayout,
    components,
    sessionId,
    feedStateForComponent,
    parentType = null,
    siblingCount = 1,
    distribution,
  }: Props = $props();

  const isDashboard = $derived(surfaceUsesDashboardFill(surfaceLayout));

  function configString(config: Record<string, unknown>, key: string): string | null {
    const camel = config[key];
    if (typeof camel === "string" && camel.trim()) return camel.trim();
    const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
    return typeof snake === "string" && snake.trim() ? snake.trim() : null;
  }

  function fillContext(
    parent: "vstack" | "hstack" | "grid",
    count: number,
    dist?: LayoutFillContext["distribution"],
    flex?: number | null,
  ): LayoutFillContext {
    return {
      surfaceLayout,
      parentType: parent,
      siblingCount: count,
      distribution: dist,
      flex,
    };
  }
</script>

{#if node.type === "vstack"}
  <div
    class="layout-node layout-node-vstack"
    class:layout-node-fill={isDashboard}
    style:display="flex"
    style:flex-direction="column"
    style:gap={spacingToGap(node.spacing)}
    style:align-items={stackCrossAlign(node.align, "column", {
      dashboard: isDashboard,
      distribution: node.distribution,
    })}
    style:justify-content={distributionToJustify(node.distribution)}
    style:min-height="0"
    style:flex={isDashboard ? "1 1 auto" : undefined}
  >
    {#each node.children as child, index (index)}
      <Self
        node={child}
        {surfaceId}
        {surfaceLayout}
        {components}
        {sessionId}
        {feedStateForComponent}
        parentType="vstack"
        siblingCount={node.children.length}
        distribution={node.distribution}
      />
    {/each}
  </div>
{:else if node.type === "hstack"}
  <div
    class="layout-node layout-node-hstack"
    class:layout-node-fill={isDashboard}
    style:display="flex"
    style:flex-direction="row"
    style:gap={spacingToGap(node.spacing)}
    style:align-items={stackCrossAlign(node.align, "row", {
      dashboard: isDashboard,
      distribution: node.distribution,
    })}
    style:justify-content={distributionToJustify(node.distribution)}
    style:min-height="0"
    style:min-width="0"
    style:flex={isDashboard ? "1 1 auto" : undefined}
  >
    {#each node.children as child, index (index)}
      <Self
        node={child}
        {surfaceId}
        {surfaceLayout}
        {components}
        {sessionId}
        {feedStateForComponent}
        parentType="hstack"
        siblingCount={node.children.length}
        distribution={node.distribution}
      />
    {/each}
  </div>
{:else if node.type === "grid"}
  <div
    class="layout-node layout-node-grid"
    class:layout-node-fill={isDashboard}
    style:display="grid"
    style:grid-template-columns={`repeat(${node.columns}, minmax(0, 1fr))`}
    style:gap={spacingToGap(node.spacing)}
    style:min-height="0"
    style:flex={isDashboard ? "1 1 auto" : undefined}
  >
    {#each node.children as child, index (index)}
      <Self
        node={child}
        {surfaceId}
        {surfaceLayout}
        {components}
        {sessionId}
        {feedStateForComponent}
        parentType="grid"
        siblingCount={node.children.length}
      />
    {/each}
  </div>
{:else if node.type === "component"}
  {@const component = componentById(components, node.id)}
  {@const ctx = fillContext(parentType ?? "vstack", siblingCount, distribution, node.flex)}
  {#if component}
    <div
      class="environment-renderer-main-item"
      class:environment-renderer-main-item-fill={isDashboard}
      style:flex={flexStyle(node.flex, distribution)}
      style:min-height="0"
      style:min-width="0"
      style:height={isDashboard && parentType === "hstack" ? "100%" : undefined}
    >
      {#if component.type === "presentation"}
        {@const artifactId = configString(component.config, "artifactId")}
        {@const embedMode = presentationEmbedMode(surfaceLayout, component, ctx)}
        {#if artifactId && sessionId}
          <PresentationFrame
            {sessionId}
            artifactId={artifactId}
            label={component.label ?? "Presentation"}
            mode={embedMode}
            bare={presentationBare(surfaceLayout, embedMode, ctx)}
            feedState={feedStateForComponent(component.id)}
            subscribedFeedIds={component.feeds ?? []}
          />
        {/if}
      {:else if component.type === "medousa_view"}
        {@const notePath = configString(component.config, "notePath")}
        {#if notePath}
          <EnvironmentMedousaView
            {notePath}
            fill={presentationEmbedMode(surfaceLayout, component, ctx) === "panel"}
          />
        {/if}
      {:else if component.type === "chrome_action"}
        <ChromeActionRenderer {component} variant="inline" />
      {:else}
        <p class="environment-renderer-unsupported">
          Component <code>{component.id}</code> ({component.type}) is not supported in layout tree.
        </p>
      {/if}
    </div>
  {:else}
    <p class="environment-renderer-unsupported">
      Missing component <code>{node.id}</code> for layout tree.
    </p>
  {/if}
{/if}

<style>
  .layout-node-fill {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
  }

  .environment-renderer-main-item-fill {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    width: 100%;
  }

  .environment-renderer-main-item-fill :global(.presentation-frame) {
    flex: 1 1 auto;
    min-height: 0;
  }

  .environment-renderer-main-item-fill :global(.presentation-frame-fill) {
    height: 100%;
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
