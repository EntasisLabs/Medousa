<script lang="ts">
  import { onDestroy, type Component } from "svelte";
  import ShellChunkError from "$lib/components/layout/ShellChunkError.svelte";

  let {
    loader,
    overlay = false,
    onSettled,
    ...rest
  }: {
    loader: (signal?: AbortSignal) => Promise<{
      default: unknown;
      release?: () => void;
    }>;
    overlay?: boolean;
    onSettled?: () => void;
    [key: string]: unknown;
  } = $props();

  let controller = new AbortController();
  let loaded: { release?: () => void } | undefined;
  let pending = $state(load());

  async function load() {
    try {
      const result = await loader(controller.signal);
      loaded = result;
      return result;
    } finally {
      onSettled?.();
    }
  }

  function retry() {
    controller.abort("retry");
    loaded?.release?.();
    loaded = undefined;
    controller = new AbortController();
    pending = load();
  }

  onDestroy(() => {
    controller.abort("navigate-away");
    loaded?.release?.();
  });
</script>

{#await pending}
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
      onRetry={retry}
    />
{/await}
