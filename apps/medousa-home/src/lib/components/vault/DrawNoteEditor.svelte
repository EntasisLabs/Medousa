<script lang="ts">
  import DrawSurface from "$lib/components/draw/DrawSurface.svelte";
  import { untrack } from "svelte";
  import {
    drawDocumentFromContent,
    encodeDrawDocument,
    replaceDrawFence,
    type DrawDocument,
  } from "$lib/draw/drawDocument";

  interface Props {
    content: string;
    disabled?: boolean;
    onchange: (nextContent: string) => void;
  }

  let { content, disabled = false, onchange }: Props = $props();
  let sourceContent = $state(untrack(() => content));
  let document = $state(untrack(() => drawDocumentFromContent(content)));
  let syncedContent = $state(untrack(() => content));

  $effect(() => {
    if (content === syncedContent) return;
    sourceContent = content;
    document = drawDocumentFromContent(content);
    syncedContent = content;
  });

  function handleChange(next: DrawDocument) {
    const markdown = replaceDrawFence(sourceContent, next);
    sourceContent = markdown;
    syncedContent = markdown;
    document = next;
    onchange(markdown);
  }
</script>

<div class="draw-note-editor" data-draw-fingerprint={encodeDrawDocument(document)}>
  <DrawSurface {document} editable={!disabled} variant="full" onchange={handleChange} />
</div>

<style>
  .draw-note-editor { display: flex; min-width: 0; min-height: 0; flex: 1; overflow: hidden; }
</style>
