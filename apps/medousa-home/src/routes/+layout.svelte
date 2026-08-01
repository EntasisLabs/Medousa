<script lang="ts">
  import "../app.postcss";
  import { onMount } from "svelte";
  import { initializeStores } from "@skeletonlabs/skeleton";
  import { settings } from "$lib/stores/settings.svelte";

  initializeStores();
  settings.applyTheme();

  onMount(() => {
    const bootstrapSplash = document.getElementById("medousa-bootstrap-splash");
    if (!bootstrapSplash) return;
    bootstrapSplash.setAttribute("data-exiting", "true");
    window.setTimeout(() => bootstrapSplash.remove(), 220);
  });

  let { children } = $props();
</script>

<div class="h-full">
  {@render children()}
</div>
