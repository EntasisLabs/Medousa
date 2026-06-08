<script lang="ts">
  import { onMount } from "svelte";
  import ActivitySheet from "$lib/components/mobile/ActivitySheet.svelte";
  import AskSheet from "$lib/components/mobile/AskSheet.svelte";
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
  import { chat } from "$lib/stores/chat.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { ensureNotificationPermission } from "$lib/notifications";
  import { setMobileBadge } from "$lib/mobileBadge";
  import { isTauri, updateTrayBlockedCount } from "$lib/window";
  import {
    connectWorkshop,
    reconnectWorkshop,
  } from "$lib/workshopConnection";
  import type { DaemonHealth } from "$lib/daemon";

  let daemonHealth = $state<DaemonHealth | null>(null);

  $effect(() => {
    const blocked = workspace.blockedCount();
    void setMobileBadge(blocked);
    if (isTauri()) {
      void updateTrayBlockedCount(blocked);
    }
  });

  $effect(() => {
    if (layout.mobileTab === "chat" && daemonHealth?.ok) {
      void chat.ensureSessionHydrated();
    }
  });

  onMount(() => {
    void ensureNotificationPermission();
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
  <main class="min-h-0 flex-1 overflow-hidden">
    {#if layout.mobileTab === "pulse"}
      <PulsePanel
        health={daemonHealth}
        onSelectCard={handleSelectCard}
        onOpenChat={handleOpenChat}
        onOpenSettings={() => layout.openYou("settings")}
        onToggleActivity={() => layout.toggleActivitySheet()}
      />
    {:else if layout.mobileTab === "work"}
      <WorkTimeline visible={true} onSelectCard={handleSelectCard} />
    {:else if layout.mobileTab === "chat"}
      <ChatPanel visible={true} showPopout={false} mobile={true} />
    {:else}
      <YouHub
        visible={true}
        health={daemonHealth}
        revision={workspace.revision}
        onOpenChat={handleOpenChat}
        onDaemonHealth={async () => {
          daemonHealth = await reconnectWorkshop((health) => {
            daemonHealth = health;
          });
        }}
      />
    {/if}
  </main>

  <MobileTabBar />

  <SessionSidebar
    variant="sheet"
    open={layout.mobileTab === "chat" && layout.sessionDrawerOpen}
    onClose={() => layout.setSessionDrawerOpen(false)}
  />
  <AskSheet />
  <IdentityDrawer
    variant="sheet"
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
