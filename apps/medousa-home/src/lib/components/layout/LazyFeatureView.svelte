<script lang="ts">
  import type { Component } from "svelte";
  import ShellChunkError from "$lib/components/layout/ShellChunkError.svelte";

  let {
    loader,
    overlay = false,
    ...rest
  }: {
    loader: () => Promise<{ default: unknown }>;
    overlay?: boolean;
    [key: string]: unknown;
  } = $props();

  let epoch = $state(0);
</script>

{#key epoch}
  {#await loader()}
    {#if !overlay}
      <div class="flex h-full items-center justify-center p-8 text-sm text-content-quiet">
        Loading…
      </div>
    {/if}
  {:then mod}
    {@const View = mod.default as Component<Record<string, unknown>>}
    <View {...rest} />
  {:catch}
    <ShellChunkError
      onRetry={() => {
        epoch += 1;
      }}
    />
  {/await}
{/key}
