<script lang="ts">
  import { onMount } from "svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { hideChatPopout, isTauri } from "$lib/window";
  import { onInteractiveEvent, onInteractiveError } from "$lib/daemon";
  import { isRecoverableStreamError } from "$lib/utils/streamEvents";
  import type { InteractiveTurnStreamEvent } from "$lib/types/chat";

  onMount(() => {
    settings.applyTheme();
    const unlisteners: Promise<() => void>[] = [];

    (async () => {
      await Promise.all([
        chat.refreshSessions({ force: true }),
        chat.reloadCurrentSession(),
      ]);

      unlisteners.push(
        onInteractiveEvent<InteractiveTurnStreamEvent>((event) => {
          chat.applyStreamEvent(event);
        }),
      );
      unlisteners.push(
        onInteractiveError((message) =>
          chat.noteStreamFailure(message, {
            recoverable: isRecoverableStreamError(message),
          }),
        ),
      );
    })();

    return () => {
      Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  });

  async function handleClose() {
    if (isTauri()) {
      await hideChatPopout();
    }
  }
</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <header
    class="flex items-center justify-between border-b border-surface-500/20 px-4 py-2"
  >
    <div>
      <h1 class="text-sm font-semibold">Medousa Chat</h1>
      <p class="text-xs text-surface-400">Pop-out session</p>
    </div>
    {#if isTauri()}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={handleClose}
      >
        Close
      </button>
    {/if}
  </header>

  <div class="flex min-h-0 flex-1">
    <SessionSidebar open={true} variant="inline" />
    <ChatPanel visible={true} showPopout={false} />
  </div>
</div>
