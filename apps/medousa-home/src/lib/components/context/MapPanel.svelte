<script lang="ts">
  import ContextMapView from "$lib/components/context/ContextMapView.svelte";
  import { listVaultNotes } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import { contextShell } from "$lib/stores/contextShell.svelte";
  import { contextThreads } from "$lib/stores/contextThreads.svelte";
  import type { VaultNote } from "$lib/types/vault";
  import type { ContextMapNode } from "$lib/utils/contextMap";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  const search = $derived(contextShell.search);
  const selectedMapNodeId = $derived(contextShell.selectedMapNodeId);

  let vaultNotes = $state<VaultNote[]>([]);
  let vaultError = $state<string | null>(null);

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
    void contextThreads.refresh({ limit: 200 });
    void chat.refreshSessions();
    void (async () => {
      try {
        vaultError = null;
        const response = await listVaultNotes({ limit: 200 });
        vaultNotes = response.notes;
      } catch (err) {
        vaultError = err instanceof Error ? err.message : String(err);
        vaultNotes = [];
      }
    })();
  });

  function focusMapNode(node: ContextMapNode) {
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

  const mapError = $derived(contextThreads.error ?? vaultError);
</script>

<section
  class="map-panel flex h-full min-h-0 flex-col"
  class:opacity-40={!visible}
  data-debug-label="map-panel"
>
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if visible}
      <ContextMapView
        nodes={contextThreads.nodes}
        {vaultNotes}
        {sessionLabels}
        {search}
        loading={contextThreads.loading}
        error={mapError}
        selectedNodeId={selectedMapNodeId}
        density="default"
        onFocusNode={focusMapNode}
        onClearSelection={clearMapFocus}
      />
    {/if}
  </div>
</section>
