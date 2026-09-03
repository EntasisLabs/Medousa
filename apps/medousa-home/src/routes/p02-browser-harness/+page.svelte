<script lang="ts">
  import { onMount } from "svelte";

  onMount(() => {
    void import("$lib/bench/p02BrowserHarness");
  });
</script>

<svelte:head>
  <title>P02 browser streaming render harness</title>
</svelte:head>

<div class="p02-browser-harness-page">
  <div id="status">Running P02…</div>
  <pre id="result"></pre>
  <main id="host"></main>
</div>

<style>
  .p02-browser-harness-page {
    box-sizing: border-box;
    min-height: 100%;
    overflow: auto;
    padding: 24px;
    font: 14px system-ui, sans-serif;
  }

  #status {
    position: sticky;
    z-index: 2;
    top: 0;
    padding: 8px;
    background: white;
  }

  #host {
    max-width: 760px;
  }

  pre {
    white-space: pre-wrap;
  }
</style>
