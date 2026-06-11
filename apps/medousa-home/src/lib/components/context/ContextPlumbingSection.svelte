<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    label?: string;
    children: Snippet;
  }

  let { label = "Technical details", children }: Props = $props();
</script>

<section class="context-plumbing">
  <p class="context-plumbing-label">{label}</p>
  <div class="mt-2 space-y-2">
    {@render children()}
  </div>
</section>
