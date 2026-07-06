<script lang="ts">
  import TilingLayoutView from "$lib/components/environment/TilingLayoutView.svelte";
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";
  import type { ComponentDef } from "$lib/types/environment";

  interface Props {
    surfaceId: string;
    components: ComponentDef[];
    sessionId: string;
    profileId?: string | null;
    feedStateForComponent: (componentId: string) => Record<string, unknown> | null;
    padded?: boolean;
  }

  let {
    surfaceId,
    components,
    sessionId,
    profileId = null,
    feedStateForComponent,
    padded = true,
  }: Props = $props();

  const root = $derived(layoutEdit.tilingRoot);
  const editComponents = $derived(
    components.filter((component) => !layoutEdit.removedDuringEdit.includes(component.id)),
  );
</script>

{#if root && layoutEdit.isEditingSurface(surfaceId)}
  <TilingLayoutView
    {root}
    components={editComponents}
    {sessionId}
    {profileId}
    {feedStateForComponent}
    editing={true}
    {padded}
  />
{/if}
