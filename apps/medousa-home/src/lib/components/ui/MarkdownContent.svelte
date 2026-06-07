<script lang="ts">
  import { renderMarkdownPreview } from "$lib/utils/markdownPreview";

  interface Props {
    content: string;
  }

  let { content }: Props = $props();

  const html = $derived(renderMarkdownPreview(content));
</script>

<div class="markdown-content text-sm leading-relaxed">
  {@html html}
</div>
