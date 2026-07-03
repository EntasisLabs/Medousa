<script lang="ts">
  import PresentationFrame from "$lib/components/environment/PresentationFrame.svelte";
  import type { ArtifactEmbedMode } from "$lib/utils/artifactPrepareHtml";

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
    onOpenFull,
    contentHeight = $bindable(0),
    truncated = $bindable(false),
  }: Props = $props();
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
  {onOpenFull}
  bind:contentHeight
  bind:truncated
/>
