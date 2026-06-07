<script lang="ts">
  import { onMount } from "svelte";
  import { Bell } from "@lucide/svelte";
  import ActivitySheet from "$lib/components/mobile/ActivitySheet.svelte";
  import ConnectionPill from "$lib/components/mobile/ConnectionPill.svelte";
  import MobileTabBar from "$lib/components/mobile/MobileTabBar.svelte";
  import PulsePanel from "$lib/components/mobile/PulsePanel.svelte";
  import WorkStory from "$lib/components/mobile/WorkStory.svelte";
  import WorkTimeline from "$lib/components/mobile/WorkTimeline.svelte";
  import YouHub from "$lib/components/mobile/YouHub.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import IdentityDrawer from "$lib/components/chat/IdentityDrawer.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import AskCompletionModal from "$lib/components/work/AskCompletionModal.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { isTauri, updateTrayBlockedCount } from "$lib/window";
  import {
    connectWorkshop,
    refreshDaemonHealth,
  } from "$lib/workshopConnection";
  import type { DaemonHealth } from "$lib/daemon";

  let daemonHealth = $state<DaemonHealth | null>(null);

  $effect(() => {
    if (!isTauri()) return;
    void updateTrayBlockedCount(workspace.blockedCount());
  });

  onMount(() => {
    return connectWorkshop({
      onHealthChange: (health) => {
        daemonHealth = health;
      },
    });
  });

  async function handleOpenNote(path: string) {
    await vault.openNote(path);
    layout.openYou("library");
  }

  async function handleSelectCard(id: string) {
    layout.setMobileTab("work");
    await workspace.selectCard(id);
  }

  function handleOpenChat() {
    layout.setMobileTab("chat");
  }

  function closeWorkStory() {
    workspace.clearSelection();
  }
</script>

<div class="mobile-shell flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <header class="mobile-top-bar flex shrink-0 items-center gap-2 px-4 py-2">
    <ConnectionPill
      health={daemonHealth}
      onTap={() => layout.openYou("settings")}
    />
    <button
      type="button"
      class="mobile-icon-btn shrink-0"
      aria-label="Activity"
      onclick={() => layout.toggleActivitySheet()}
    >
      <Bell size={20} strokeWidth={1.75} />
    </button>
  </header>

  <main class="min-h-0 flex-1 overflow-hidden">
    {#if layout.mobileTab === "pulse"}
      <PulsePanel
        onSelectCard={handleSelectCard}
        onOpenChat={handleOpenChat}
        onOpenNote={handleOpenNote}
      />
    {:else if layout.mobileTab === "work"}
      <WorkTimeline visible={true} onSelectCard={handleSelectCard} />
    {:else if layout.mobileTab === "chat"}
      <ChatPanel visible={true} />
    {:else}
      <YouHub
        visible={true}
        health={daemonHealth}
        revision={workspace.revision}
        onOpenChat={handleOpenChat}
        onDaemonHealth={async () => {
          daemonHealth = await refreshDaemonHealth();
        }}
      />
    {/if}
  </main>

  <MobileTabBar />

  <SessionSidebar
    open={layout.mobileTab === "chat" && layout.sessionDrawerOpen}
    onClose={() => layout.setSessionDrawerOpen(false)}
  />
  <IdentityDrawer
    open={layout.mobileTab === "chat" && layout.identityDrawerOpen}
    onClose={() => layout.setIdentityDrawerOpen(false)}
  />

  <ActivitySheet onOpenNote={handleOpenNote} />

  {#if layout.mobileTab === "work"}
    <WorkStory
      onOpenNote={handleOpenNote}
      onOpenChat={handleOpenChat}
      onClose={closeWorkStory}
    />
  {/if}

  <AskCompletionModal
    pending={workspace.pendingAskCompletion}
    onOpenNote={handleOpenNote}
    onClose={() => workspace.clearPendingAskCompletion()}
  />
</div>
