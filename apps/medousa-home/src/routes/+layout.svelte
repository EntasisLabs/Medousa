<script lang="ts">
  import "../app.postcss";
  import { onMount } from "svelte";
  import { setLucideProps } from "@lucide/svelte";
  import { initializeStores } from "@skeletonlabs/skeleton";
  import { settings } from "$lib/stores/settings.svelte";
  import { installCspViolationDiagnostics } from "$lib/security/cspDiagnostics";
  import { dismissBootstrapSplash } from "$lib/runtime/bootstrapSplash";

  initializeStores();
  settings.applyTheme();
  // Default for icons that omit strokeWidth; CSS --icon-stroke covers explicit props.
  setLucideProps({ strokeWidth: 2.15 });

  onMount(() => {
    installCspViolationDiagnostics();
    // AppShell dismisses the root splash only once its destination has mounted.
    // Utility/pop-out routes do not mount AppShell, so hand those off here.
    if (window.location.pathname !== "/") dismissBootstrapSplash();
  });

  let { children } = $props();
</script>

<div class="h-full w-full min-h-0 min-w-0">
  {@render children()}
</div>
