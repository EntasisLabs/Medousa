<script lang="ts">
  import "$lib/styles/context.postcss";
  import ContextMapView from "$lib/components/context/ContextMapView.svelte";
  import MapSidePanel from "$lib/components/context/MapSidePanel.svelte";
  import { List } from "@lucide/svelte";
  import ContextBrowse from "$lib/components/context/ContextBrowse.svelte";
  import { contextNotes } from "$lib/context/contextNotes.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextShell } from "$lib/context/contextShell.svelte";
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { contextBrowseEntries, relatedContextNotes } from "$lib/utils/contextBrowse";
  import type { ContextMapNode } from "$lib/utils/contextMap";

  interface Props {
    visible: boolean;
    mobile?: boolean;
  }

  let { visible, mobile = false }: Props = $props();

  let mobileListOpen = $state(false);
  const showMobileList = $derived(mobile && (mobileListOpen || (contextShell.lens === "browse" && !contextShell.selectedMapNodeId)));

  const search = $derived(contextShell.search);
  const selectedMapNodeId = $derived(contextShell.selectedMapNodeId);

  const vaultNotes = $derived(relatedContextNotes(contextNotes.notes, contextThreads.nodes.map((node) => node.session_id)));

  const sessionLabels = $derived(
    Object.fromEntries(
      chat.sessions.map((session) => [
        session.session_id,
        session.display_name?.trim() || session.session_id,
      ]),
    ),
  );
  $effect(() => {
    if (!visible) return;
    chat.workshopScopeId;
    void contextThreads.refresh({ limit: 200 }).then(() => {
      if (!mobile && contextShell.lens === "browse" && !contextShell.selectedMapNodeId) {
        const first = contextBrowseEntries(contextThreads.nodes, sessionLabels, contextShell.search)[0];
        if (first) contextShell.selectMapNode(first.id);
      }
    });
    void chat.refreshSessions();
    void contextNotes.refresh();
  });

  function focusMapNode(node: ContextMapNode) {
    if (node.kind === "note" && node.notePath) {
      void lmeWorkspace.openNote(node.notePath);
      return;
    }
    contextShell.selectMapNode(node.id);
    if (node.kind === "thread" && node.syncKey) {
      void contextThreads.loadDetail(node.syncKey);
    } else {
      contextThreads.clearDetail();
    }
  }

  function clearMapFocus() {
    contextShell.selectMapNode(null);
    contextThreads.clearDetail();
  }

  const mapError = $derived(contextThreads.error);
</script>

<section
  class="map-panel flex h-full min-h-0 flex-col"
  class:opacity-40={!visible}
  data-debug-label="map-panel"
>
  <header class="flex shrink-0 items-center justify-between gap-3 border-b border-surface-700/40 px-5 py-3">
    {#if mobile}
      <button type="button" class="inline-flex items-center gap-2 text-xs text-content-secondary" onclick={() => mobileListOpen = !showMobileList}><List size={14} />Saved memory</button>
    {:else}
      <p class="text-xs text-content-quiet">Context / {contextShell.lens === "map" ? "Map" : "Saved memory"}</p>
    {/if}
    <div class="inline-flex rounded-lg bg-surface-800/70 p-1" role="group" aria-label="Context lens">
      <button class="context-lens" class:active={contextShell.lens === "browse"} aria-pressed={contextShell.lens === "browse"} onclick={() => { contextShell.lens = "browse"; mobileListOpen = false; }}>Browse</button>
      <button class="context-lens" class:active={contextShell.lens === "map"} aria-pressed={contextShell.lens === "map"} onclick={() => { contextShell.lens = "map"; mobileListOpen = false; }}>Map</button>
    </div>
  </header>
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if visible && showMobileList}
      <MapSidePanel onPick={() => mobileListOpen = false} />
    {:else if visible && contextShell.lens === "browse"}
      <ContextBrowse {sessionLabels} />
    {:else if visible}
      <ContextMapView
        nodes={contextThreads.nodes}
        {vaultNotes}
        {sessionLabels}
        {search}
        loading={contextThreads.loading}
        error={mapError}
        selectedNodeId={selectedMapNodeId}
        density="default"
        momentDetail={contextThreads.detail}
        momentDetailLoading={contextThreads.detailLoading}
        onFocusNode={focusMapNode}
        onClearSelection={clearMapFocus}
      />
    {/if}
  </div>
</section>

<style>
  .context-lens { border-radius: .4rem; padding: .35rem .8rem; font-size: .75rem; color: rgb(var(--theme-text-quiet)); }
  .context-lens.active { background: rgb(var(--color-surface-700) / .65); color: rgb(var(--color-surface-100)); }
</style>
