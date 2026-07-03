<script lang="ts">
  import type { Snippet } from "svelte";
  import LayoutEditToolbar from "$lib/components/environment/LayoutEditToolbar.svelte";
  import { layoutEdit } from "$lib/stores/layoutEdit.svelte";

  interface Props {
    surfaceId: string;
    editing?: boolean;
    children?: Snippet;
  }

  let { surfaceId, editing = false, children }: Props = $props();

  const isEditing = $derived(editing || layoutEdit.isEditingSurface(surfaceId));
</script>

<div class="layout-edit-overlay" class:layout-edit-overlay-active={isEditing}>
  {#if isEditing}
    <LayoutEditToolbar {surfaceId} />
  {/if}
  {#if children}
    {@render children()}
  {/if}
</div>

<style>
  .layout-edit-overlay {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .layout-edit-overlay-active {
    outline: 1px dashed color-mix(in srgb, var(--color-primary-400) 35%, transparent);
    outline-offset: -1px;
  }
</style>
