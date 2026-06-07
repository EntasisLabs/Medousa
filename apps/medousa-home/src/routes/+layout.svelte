<script lang="ts">
  import "../app.postcss";
  import { initializeStores } from "@skeletonlabs/skeleton";
  import { settings } from "$lib/stores/settings.svelte";

  initializeStores();
  settings.applyTheme();

  let { children } = $props();
</script>

<div class="h-full">
  {@render children()}
</div>
