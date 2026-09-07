<script lang="ts">
  import { governedBrowser } from "$lib/stores/governedBrowser.svelte";
  import type { BotWorldBinding } from "$lib/types/generated/daemon_api";

  interface Props {
    binding: BotWorldBinding | null;
    disabled?: boolean;
  }

  let { binding = $bindable(), disabled = false }: Props = $props();

  const currentPersistentBrowserWorld = $derived.by(() => {
    const world = governedBrowser.selectedWorld;
    return world?.profile.kind === "persistent" ? world : null;
  });

  function bindingLabel(value: BotWorldBinding): string {
    const world = governedBrowser.worlds.find(
      (candidate) =>
        candidate.world_id === value.world_id &&
        candidate.execution_runtime_id === value.execution_runtime_id,
    );
    const title = world?.title?.trim() || "Persistent browser";
    const runtime =
      governedBrowser.runtimeLabels[value.execution_runtime_id]?.trim() ||
      value.execution_runtime_id;
    return `${title} · ${runtime}`;
  }

  function keepCurrentBrowser() {
    const world = currentPersistentBrowserWorld;
    if (!world) return;
    binding = {
      kind: "persistent_browser",
      world_id: world.world_id,
      execution_runtime_id: world.execution_runtime_id,
    };
  }
</script>

{#if binding || currentPersistentBrowserWorld}
  <div class="rounded-container-token border border-surface-700/70 p-3">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="workshop-label">Browser continuity</p>
        {#if binding}
          <p class="mt-1 truncate text-xs text-surface-200">{bindingLabel(binding)}</p>
          <p class="workshop-faint mt-0.5 text-[11px]">
            This Bot can return to that browser between conversations and restarts.
          </p>
        {:else}
          <p class="workshop-faint mt-1 text-[11px]">No browser is retained by this Bot.</p>
        {/if}
      </div>
      {#if binding}
        <button
          type="button"
          class="btn btn-xs shrink-0 variant-ghost-surface"
          {disabled}
          onclick={() => (binding = null)}
        >
          Remove
        </button>
      {:else}
        <button
          type="button"
          class="btn btn-xs shrink-0 variant-soft-surface"
          disabled={disabled || !currentPersistentBrowserWorld}
          onclick={keepCurrentBrowser}
        >
          Keep current
        </button>
      {/if}
    </div>
    {#if binding && currentPersistentBrowserWorld && binding.world_id !== currentPersistentBrowserWorld.world_id}
      <button
        type="button"
        class="workshop-faint mt-2 text-[11px] hover:text-surface-200"
        {disabled}
        onclick={keepCurrentBrowser}
      >
        Use the browser open now instead
      </button>
    {/if}
  </div>
{/if}
