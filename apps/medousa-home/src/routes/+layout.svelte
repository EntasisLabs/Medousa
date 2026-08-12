<script lang="ts">
  import "../app.postcss";
  import { onMount } from "svelte";
  import { setLucideProps } from "@lucide/svelte";
  import { initializeStores } from "@skeletonlabs/skeleton";
  import { settings } from "$lib/stores/settings.svelte";

  initializeStores();
  settings.applyTheme();
  // Default for icons that omit strokeWidth; CSS --icon-stroke covers explicit props.
  setLucideProps({ strokeWidth: 2.15 });

  onMount(() => {
    const bootstrapSplash = document.getElementById("medousa-bootstrap-splash");
    if (!bootstrapSplash) return;
    bootstrapSplash.setAttribute("data-exiting", "true");
    window.setTimeout(() => bootstrapSplash.remove(), 300);
  });

  let { children } = $props();
</script>

<div class="h-full w-full min-h-0 min-w-0">
  {@render children()}
</div>
