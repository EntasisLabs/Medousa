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
  {onOpenFull}
  bind:contentHeight
  bind:truncated
/>
