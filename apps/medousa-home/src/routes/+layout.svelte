<script lang="ts">
  import "../app.postcss";
  import { initializeStores } from "@skeletonlabs/skeleton";

  initializeStores();

  let { children } = $props();
</script>

<div class="h-full">
  {@render children()}
</div>
