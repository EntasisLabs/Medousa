<script lang="ts">
  import { onMount } from "svelte";
  import EnvironmentRenderer from "$lib/components/environment/EnvironmentRenderer.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import {
    VIEW_POPOUT_SURFACE_KEY,
    readViewPopoutSurface,
  } from "$lib/utils/viewPopout";
  import {
    hideViewPopout,
    isTauri,
    setViewPopoutWindowTitle,
  } from "$lib/window";
  import { connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";
  import { X } from "@lucide/svelte";

  let surfaceId = $state<string | null>(null);

  const surface = $derived(surfaceId ? environment.surfaceById(surfaceId) : null);
  const title = $derived(surface?.label?.trim() || "Medousa View");

  $effect(() => {
    if (!isTauri()) return;
    void setViewPopoutWindowTitle(title);
  });

  onMount(() => {
    const detachWorkshop = whenDocumentVisible(() => {
      surfaceId = readViewPopoutSurface();
      return connectWorkshop({
        onHealthChange: () => {},
        mode: "observer",
      });
    });

    function onStorage(event: StorageEvent) {
      if (event.key !== VIEW_POPOUT_SURFACE_KEY) return;
      if (document.visibilityState !== "visible") return;
      surfaceId = readViewPopoutSurface();
    }
    window.addEventListener("storage", onStorage);

    return () => {
      window.removeEventListener("storage", onStorage);
      detachWorkshop();
    };
  });

  async function handleClose() {
    if (isTauri()) await hideViewPopout();
  }
</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <header
    class="flex items-center justify-between border-b border-surface-500/20 px-4 py-2"
    data-tauri-drag-region
  >
    <div class="min-w-0">
      <h1 class="truncate text-sm font-semibold">{title}</h1>
      <p class="text-xs text-surface-400">Custom view</p>
    </div>
    {#if isTauri()}
      <button
        type="button"
        class="inline-flex size-8 items-center justify-center rounded-md text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-100"
        aria-label="Close view window"
        onclick={() => void handleClose()}
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    {/if}
  </header>

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if surfaceId && surface}
      <EnvironmentRenderer {surfaceId} />
    {:else if surfaceId}
      <div class="flex h-full items-center justify-center px-6 text-center">
        <p class="text-sm text-surface-400">
          View “{surfaceId}” isn’t in the active layout.
        </p>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center px-6 text-center">
        <p class="text-sm text-surface-400">Pick a custom view from the desktop toolbar.</p>
      </div>
    {/if}
  </div>
</div>
