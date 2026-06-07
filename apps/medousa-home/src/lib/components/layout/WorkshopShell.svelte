<script lang="ts">
  import { onMount } from "svelte";
  import IconRail from "$lib/components/layout/IconRail.svelte";
  import type { Surface } from "$lib/types/ui";
  import WorkRail from "$lib/components/layout/WorkRail.svelte";
  import ActivityPanel from "$lib/components/layout/ActivityPanel.svelte";
  import HomeOverview from "$lib/components/layout/HomeOverview.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import LibraryPanel from "$lib/components/vault/LibraryPanel.svelte";
  import WorkPanel from "$lib/components/work/WorkPanel.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    checkDaemonHealth,
    onInteractiveEvent,
    onInteractiveError,
    onWorkspaceEvent,
    onWorkspaceError,
    startWorkspaceStream,
    stopInteractiveStream,
    stopWorkspaceStream,
  } from "$lib/daemon";
  import type { WorkspaceStreamEvent } from "$lib/types/workspace";
  import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

  let activeSurface = $state<Surface>("chat");
  let daemonMessage = $state<string | null>(null);

  onMount(() => {
    const unlisteners: Promise<() => void>[] = [];

    (async () => {
      const health = await checkDaemonHealth();
      daemonMessage = health.message;

      await stopWorkspaceStream();
      await startWorkspaceStream(workspace.revision || undefined);

      unlisteners.push(
        onWorkspaceEvent<WorkspaceStreamEvent>((event) => {
          workspace.applyEvent(event);
          const kind = event.feed_event?.kind;
          if (
            kind === "vault_note_created" ||
            kind === "vault_note_updated"
          ) {
            void vault.refreshNotes();
            if (
              vault.selectedPath &&
              event.feed_event?.summary.includes(vault.selectedPath)
            ) {
              void vault.openNote(vault.selectedPath);
            }
          }
        }),
      );
      unlisteners.push(
        onWorkspaceError((message) => workspace.setError(message)),
      );
      unlisteners.push(
        onInteractiveEvent<InteractiveTurnStreamEvent>((event) => {
          chat.applyStreamEvent(event);
        }),
      );
      unlisteners.push(
        onInteractiveError((message) => chat.setError(message)),
      );
    })();

    return () => {
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
      stopWorkspaceStream();
      stopInteractiveStream();
    };
  });

  function handleSurfaceSelect(surface: Surface) {
    if (surface === "settings") return;
    activeSurface = surface;
    if (surface === "work" && workspace.workView === "kanban") {
      void workspace.prefetchCardDetails();
    }
  }

  async function handleOpenNote(path: string) {
    activeSurface = "library";
    await vault.openNote(path);
  }

  async function handleCardSelect(id: string) {
    activeSurface = "work";
    await workspace.selectCard(id);
  }

</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <div class="flex min-h-0 flex-1">
    <IconRail active={activeSurface} onSelect={handleSurfaceSelect} />

    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-0 flex-1">
        {#if activeSurface === "home"}
          <HomeOverview onOpenWork={() => (activeSurface = "work")} />
        {:else if activeSurface === "library"}
          <LibraryPanel visible={true} />
        {:else if activeSurface === "work"}
          <WorkPanel
            visible={true}
            onOpenNote={handleOpenNote}
            onOpenChat={() => (activeSurface = "chat")}
            onSelectCard={handleCardSelect}
          />
        {:else}
          <ChatPanel visible={activeSurface === "chat"} />
        {/if}

        <ActivityPanel
          events={workspace.feed}
          error={workspace.streamError}
          {daemonMessage}
          notePath={vault.selectedPath}
          noteTitle={vault.title}
          wikilinksOut={vault.wikilinksOut}
          backlinks={vault.backlinks}
          cardDetail={workspace.selectedCardDetail}
          cardError={workspace.cardDetailError}
          onOpenNote={handleOpenNote}
        />
      </div>

      <WorkRail
        cards={workspace.activeCards()}
        selectedId={workspace.selectedCardId}
        onSelect={handleCardSelect}
      />
    </div>
  </div>
</div>
