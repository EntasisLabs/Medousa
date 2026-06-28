<script lang="ts">
  import { onMount } from "svelte";
  import MobileToast from "$lib/components/mobile/MobileToast.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import ActivitySheet from "$lib/components/mobile/ActivitySheet.svelte";
  import AskSheet from "$lib/components/mobile/AskSheet.svelte";
  import MobileBottomChrome from "$lib/components/mobile/MobileBottomChrome.svelte";
  import HomePanel from "$lib/components/mobile/HomePanel.svelte";
  import WorkStory from "$lib/components/mobile/WorkStory.svelte";
  import MoreHub from "$lib/components/mobile/MoreHub.svelte";
  import MobileLibraryPanel from "$lib/components/mobile/MobileLibraryPanel.svelte";
  import BrowserPanel from "$lib/components/browser/BrowserPanel.svelte";
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
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    attachMobileKeyboardViewport,
    setMobileBrowserUrlFocus,
    setMobileComposerFocus,
    isMobileBrowserUrlFocused,
  } from "$lib/utils/mobileKeyboardViewport";
  import {
    connectWorkshop,
    reconnectWorkshop,
  } from "$lib/workshopConnection";
  import type { DaemonHealth } from "$lib/daemon";
  import { attachMobileTabSwipe } from "$lib/utils/mobileTabSwipe";
  import { switchMobileTab } from "$lib/mobileNavigation";

  let daemonHealth = $state<DaemonHealth | null>(null);
  let mainEl: HTMLElement | undefined = $state();

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

  $effect(() => {
    if (layout.mobileTab !== "chat") {
      setMobileComposerFocus(false);
    }
    if (layout.mobileTab !== "web") {
      setMobileBrowserUrlFocus(false);
    }
  });

  $effect(() => {
    if (!mainEl) return;
    return attachMobileTabSwipe(mainEl);
  });

  onMount(() => {
    void workshops.load();
    void ensureNotificationPermission();
    void vault.refreshNotes();
    const detachKeyboard = attachMobileKeyboardViewport();
    const detachWorkshop = connectWorkshop({
      onHealthChange: (health) => {
        daemonHealth = health;
      },
    });
    return () => {
      detachKeyboard();
      detachWorkshop();
    };
  });

  async function handleOpenNote(path: string) {
    await vault.openNote(path);
    vault.enterPreviewMode();
    layout.setLibraryView("reader");
    layout.openNotes();
  }

  async function handleSelectCard(id: string) {
    switchMobileTab("home");
    await workspace.selectCard(id);
  }

  async function handleOpenChat(sessionId?: string) {
    switchMobileTab("chat");
    if (sessionId) {
      await chat.switchSession(sessionId);
    }
  }

  function closeWorkStory() {
    workspace.clearSelection();
  }
</script>

<div
  class="mobile-shell flex min-h-0 w-full flex-col bg-surface-950 text-surface-50"
  data-mobile-tab={layout.mobileTab}
>
  <main bind:this={mainEl} class="min-h-0 flex-1 overflow-hidden">
    {#key layout.navigationEpoch}
      {#if layout.mobileTab === "home"}
        <HomePanel
          health={daemonHealth}
          onSelectCard={handleSelectCard}
          onOpenChat={handleOpenChat}
          onOpenNote={handleOpenNote}
          onOpenSettings={() => layout.openMore("settings")}
          onToggleActivity={() => layout.toggleActivitySheet()}
        />
      {:else if layout.mobileTab === "chat"}
        <ChatPanel
          visible={true}
          showPopout={false}
          mobile={true}
          onOpenContext={() => {
            layout.setIdentityDrawerOpen(false);
            layout.openMore("context");
          }}
          onOpenConnection={() => layout.openMore("settings")}
        />
      {:else if layout.mobileTab === "notes"}
        <MobileLibraryPanel visible={true} onOpenChat={handleOpenChat} />
      {:else if layout.mobileTab === "web"}
        <BrowserPanel visible={true} />
      {:else if layout.mobileTab === "more"}
        <MoreHub
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
    {/key}
  </main>

  <MobileBottomChrome />

  <MobileToast
    message={userProfiles.remoteChangeNotice}
    onDismiss={() => userProfiles.dismissRemoteChangeNotice()}
  />

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
    onOpenFullContext={() => {
      layout.setIdentityDrawerOpen(false);
      layout.openMore("context");
    }}
  />

  <ActivitySheet onOpenNote={handleOpenNote} />

  {#if layout.mobileTab === "home"}
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
