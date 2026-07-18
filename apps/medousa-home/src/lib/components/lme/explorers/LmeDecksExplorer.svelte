<script lang="ts">
  import { onMount } from "svelte";
  import ArtifactLibraryList from "$lib/components/artifacts/ArtifactLibraryList.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

  interface Props {
    onOpenChat: () => void;
  }

  let { onOpenChat }: Props = $props();

  onMount(() => {
    void artifacts.refresh();
  });
</script>

<aside class="lme-decks-explorer flex h-full min-h-0 w-full flex-col" aria-label="Decks">
  <div class="border-b border-surface-500/45 px-3 py-2">
    <input
      class="w-full rounded-lg border border-surface-500/45 bg-surface-900/70 px-2.5 py-1.5 text-sm text-surface-100 outline-none ring-primary-500/30 focus:ring-2"
      placeholder="Filter presentations…"
      value={artifacts.searchQuery}
      oninput={(event) => artifacts.setSearchQuery(event.currentTarget.value)}
    />
  </div>

  {#if artifacts.error}
    <p
      class="mx-3 mt-3 rounded-container-token border border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-error-300"
    >
      {artifacts.error}
    </p>
  {/if}

  {#if artifacts.loading}
    <p class="px-3 py-4 text-sm text-surface-500">Loading presentations…</p>
  {:else}
    <ArtifactLibraryList
      artifacts={artifacts.filteredArtifacts}
      selectedArtifactId={artifacts.selectedArtifactId}
      sessionTitle={(sessionId) => artifacts.sessionTitle(sessionId)}
      onSelect={(artifactId) => {
        const entry = artifacts.artifacts.find((row) => row.artifact_id === artifactId);
        lmeWorkspace.openDeck(artifactId, entry?.label);
      }}
      onOpenChat={(artifact) => {
        void artifact;
        onOpenChat();
      }}
    />
  {/if}
</aside>
