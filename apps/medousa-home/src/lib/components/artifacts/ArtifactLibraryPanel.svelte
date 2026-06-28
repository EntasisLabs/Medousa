<script lang="ts">
  import { onMount } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import ArtifactLibraryList from "./ArtifactLibraryList.svelte";
  import ArtifactLibraryPreview from "./ArtifactLibraryPreview.svelte";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

  let panelOpen = $state(false);

  onMount(() => {
    void artifacts.refresh();
  });

  function openSession(sessionId: string) {
    chat.sessionId = sessionId;
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1">
  {#if artifacts.error}
    <p class="mx-3 mt-3 rounded-container-token border border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-error-300">
      {artifacts.error}
    </p>
  {/if}

  <SplitPane
    width={layout.vaultTreeWidth}
    side="left"
    min={220}
    max={480}
    onResize={(width) => layout.setVaultTreeWidth(width)}
  >
    <aside class="flex h-full w-full flex-col border-r border-surface-500/45 bg-surface-900/40">
      <div class="border-b border-surface-500/45 px-3 py-2">
        <input
          class="w-full rounded-lg border border-surface-500/45 bg-surface-900/70 px-2.5 py-1.5 text-sm text-surface-100 outline-none ring-primary-500/30 focus:ring-2"
          placeholder="Filter presentations…"
          value={artifacts.searchQuery}
          oninput={(event) => artifacts.setSearchQuery(event.currentTarget.value)}
        />
      </div>
      {#if artifacts.loading}
        <p class="px-3 py-4 text-sm text-surface-500">Loading presentations…</p>
      {:else}
        <ArtifactLibraryList
          artifacts={artifacts.filteredArtifacts}
          selectedArtifactId={artifacts.selectedArtifactId}
          sessionTitle={(sessionId) => artifacts.sessionTitle(sessionId)}
          onSelect={(artifactId) => {
            artifacts.selectArtifact(artifactId);
            panelOpen = true;
          }}
        />
      {/if}
    </aside>
  </SplitPane>

  <ArtifactLibraryPreview
    artifact={artifacts.selectedArtifact}
    sessionTitle={artifacts.selectedArtifact
      ? artifacts.sessionTitle(artifacts.selectedArtifact.session_id)
      : ""}
    bind:panelOpen
    {onOpenChat}
    onOpenSession={openSession}
  />
</section>
