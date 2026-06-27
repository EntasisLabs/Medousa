<script lang="ts">
  import { Plus, X } from "@lucide/svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";

  interface Props {
    variant?: "bar" | "sheet";
    onSelect?: () => void;
  }

  let { variant = "bar", onSelect }: Props = $props();

  function handleNewTab() {
    void humanBrowser.openTab("about:blank");
    onSelect?.();
  }

  function handleActivate(tabId: string) {
    void humanBrowser.activateTab(tabId);
    onSelect?.();
  }

  function handleClose(tabId: string, event: Event) {
    event.preventDefault();
    event.stopPropagation();
    void humanBrowser.closeTab(tabId);
  }
</script>

{#if variant === "bar"}
  <div class="flex min-w-0 items-end gap-1 overflow-x-auto border-b border-surface-800/60 bg-surface-950/40 px-2 pt-1.5">
    {#each humanBrowser.tabs as tab (tab.id)}
      <div
        class="group flex max-w-[220px] min-w-[120px] items-center gap-1 rounded-t-md border px-2 py-1.5 text-xs {tab.active
          ? 'border-surface-600 border-b-transparent bg-surface-900 text-surface-50'
          : 'border-transparent bg-surface-900/40 text-surface-300 hover:bg-surface-900/70'}"
      >
        <button
          type="button"
          class="min-w-0 flex-1 truncate text-left"
          onclick={() => handleActivate(tab.id)}
          title={tab.url}
        >
          {tab.title || tab.url || "New tab"}
        </button>
        {#if humanBrowser.tabs.length > 1}
          <button
            type="button"
            class="ml-auto shrink-0 rounded p-0.5 text-surface-500 opacity-0 group-hover:opacity-100 hover:bg-surface-800 hover:text-surface-100"
            aria-label="Close tab"
            onclick={(event) => handleClose(tab.id, event)}
          >
            <X size={12} />
          </button>
        {/if}
      </div>
    {/each}
    <button
      type="button"
      class="mb-1 rounded-md p-1.5 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
      aria-label="New tab"
      title="New tab"
      onclick={handleNewTab}
    >
      <Plus size={14} />
    </button>
  </div>
{:else}
  <div class="flex flex-col gap-0.5 p-1.5">
    {#each humanBrowser.tabs as tab (tab.id)}
      <div
        class="flex items-center gap-2 rounded-xl px-2.5 py-2 {tab.active
          ? 'bg-surface-800/90'
          : 'hover:bg-surface-800/50'}"
      >
        <button
          type="button"
          class="min-w-0 flex-1 text-left"
          onclick={() => handleActivate(tab.id)}
        >
          <p class="truncate text-sm font-medium text-surface-50">{tab.title || "New tab"}</p>
          <p class="truncate text-[11px] text-surface-400">{tab.url === "about:blank" ? "Blank tab" : tab.url}</p>
        </button>
        {#if humanBrowser.tabs.length > 1}
          <button
            type="button"
            class="btn btn-icon btn-sm shrink-0 text-surface-400"
            aria-label="Close tab"
            onclick={(event) => handleClose(tab.id, event)}
          >
            <X size={14} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}
