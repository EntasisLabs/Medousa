<script lang="ts">
  import { onMount } from "svelte";
  import { dismissBootstrapSplash } from "$lib/runtime/bootstrapSplash";

  onMount(dismissBootstrapSplash);
</script>
