<script lang="ts">
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();

  let host = $state<HTMLDivElement | null>(null);
  let ready = $state(false);

  onMount(() => {
    if (!host) return;
    document.body.appendChild(host);
    ready = true;
    return () => {
      host?.remove();
    };
  });
</script>

<div bind:this={host} class="body-portal-host">
  {#if ready}
    {@render children()}
  {/if}
</div>

<style>
  .body-portal-host {
    display: block;
    pointer-events: none;
  }

  .body-portal-host :global(*) {
    pointer-events: auto;
  }
</style>
