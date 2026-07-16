<script lang="ts">
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import type { ArtifactEmbedMode } from "$lib/utils/artifactPrepareHtml";
  import { artifactStoreScopeId } from "$lib/utils/medousaStoreClient";

  interface Props {
    sessionId: string;
    artifactId: string;
    label: string;
    mime: string;
    heightPx?: number | null;
    compact?: boolean;
    bare?: boolean;
    mode?: ArtifactEmbedMode;
    manageable?: boolean;
    /** Prefer root lineage id for a stable MedousaStore scope across revisions. */
    rootArtifactId?: string | null;
    /** Override store scope (e.g. environment canvas component id). */
    componentId?: string | null;
    onOpenFull?: () => void;
    contentHeight?: number;
    truncated?: boolean;
  }

  let {
    sessionId,
    artifactId,
    label,
    mime,
    heightPx = 360,
    compact = false,
    bare = false,
    mode = "inline",
    manageable = false,
    rootArtifactId = null,
    componentId = null,
    onOpenFull,
    contentHeight = $bindable(0),
    truncated = $bindable(false),
  }: Props = $props();

  const storeComponentId = $derived(
    componentId ?? artifactStoreScopeId(rootArtifactId ?? artifactId),
  );
</script>

<PresentationFrame
  {sessionId}
  {artifactId}
  {label}
  {mime}
  {heightPx}
  {compact}
  {bare}
  {mode}
  {manageable}
  componentId={storeComponentId}
  {onOpenFull}
  bind:contentHeight
  bind:truncated
/>
