<script lang="ts">
  import { Globe, Plus } from "@lucide/svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { faviconUrlForSite, hostnameFromUrl, tabDisplayLabel } from "$lib/utils/browserFavicon";

  interface Props {
    onPickTab?: (tabId: string) => void;
    chrome?: "default" | "rail-list";
  }

  let { onPickTab, chrome = "rail-list" }: Props = $props();

  async function activate(tabId: string) {
    await humanBrowser.activateTab(tabId);
    onPickTab?.(tabId);
  }

  async function newTab() {
    await humanBrowser.openTab("about:blank");
    const id = humanBrowser.activeTab?.id;
    if (id) onPickTab?.(id);
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-chrome={chrome}>
  {#if humanBrowser.tabs.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-3 py-6 text-center">
      <Globe size={22} strokeWidth={1.5} class="text-surface-500" />
      <p class="text-sm text-surface-300">No open tabs</p>
      <button type="button" class="btn btn-sm btn-primary" onclick={() => void newTab()}>
        New tab
      </button>
    </div>
  {:else}
    <ul class="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5">
      {#each humanBrowser.tabs as tab (tab.id)}
        {@const host = hostnameFromUrl(tab.url)}
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-surface-800/70 {tab.active
              ? 'bg-surface-800/90 text-surface-50'
              : 'text-surface-200'}"
            title={tab.url}
            onclick={() => void activate(tab.id)}
          >
            {#if tab.url !== "about:blank"}
              <img
                src={tab.favicon || faviconUrlForSite(tab.url, 32)}
                alt=""
                class="size-4 shrink-0 rounded-sm"
              />
            {:else}
              <Plus size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" />
            {/if}
            <span class="min-w-0 flex-1">
              <span class="block truncate text-[13px] font-medium">
                {tabDisplayLabel(tab.title, tab.url)}
              </span>
              {#if host}
                <span class="block truncate text-[11px] text-surface-500">{host}</span>
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
