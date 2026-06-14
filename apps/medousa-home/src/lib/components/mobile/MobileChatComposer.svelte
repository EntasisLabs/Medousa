<script lang="ts">
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import DaemonPortalChip from "$lib/components/chat/DaemonPortalChip.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { createTurnTicket } from "$lib/daemon";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import { setMobileComposerFocus } from "$lib/utils/mobileKeyboardViewport";

  let composerBlurTimer: ReturnType<typeof setTimeout> | undefined;

  function parseDaemonAskPrompt(value: string): string | null {
    const slash = parseChatSlashInput(value);
    if (slash?.kind === "ask") return slash.prompt;
    return null;
  }

  async function submitTurn(
    userContent: string,
    prompt: string,
    mode: "interactive" | "background",
  ) {
    const opts = buildInteractiveTurnOptions();
    const mediaRefs = [...chat.pendingMediaRefs];
    const accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt,
      mode,
      provider: opts.provider,
      model: opts.model,
      responseDepthMode: opts.responseDepthMode,
      stageRouting: opts.stageRouting,
      channelSurface: opts.channelSurface,
      mediaRefs,
    });
    chat.beginTurn(userContent, accepted, mediaRefs);
    chat.clearPendingMedia();
    await chat.startTurnStream(
      accepted.turn_id,
      accepted.session_id,
      accepted.stream_url,
    );
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline) return;
    const prompt = chat.draft.trim();
    const hasAttachments = chat.pendingMediaRefs.length > 0;
    if (!prompt && !hasAttachments) return;
    haptic("medium");

    const askPrompt = parseDaemonAskPrompt(prompt);
    const slash = parseChatSlashInput(prompt);
    chat.draft = "";

    try {
      if (slash && slash.kind !== "ask") {
        await runSlashCommand(slash);
        return;
      }

      if (askPrompt) {
        await submitTurn(prompt || pendingMediaLabels(chat.pendingMediaRefs), askPrompt, "background");
        return;
      }

      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      const display =
        prompt ||
        (hasAttachments ? `[${pendingMediaLabels(chat.pendingMediaRefs)}]` : "");
      await submitTurn(display, prompt, mode);
    } catch (err) {
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit(event);
    }
  }

  function handleComposerFocus() {
    if (composerBlurTimer) {
      clearTimeout(composerBlurTimer);
      composerBlurTimer = undefined;
    }
    setMobileComposerFocus(true);
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }

  function handleComposerBlur() {
    composerBlurTimer = setTimeout(() => {
      setMobileComposerFocus(false);
      composerBlurTimer = undefined;
    }, 150);
  }
</script>

<form class="mobile-chat-composer" onsubmit={submit}>
  <BudgetApprovalBar
    mobile
    onOpenWork={() => {
      switchMobileTab("work");
      const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
      if (pending) void workspace.selectCard(pending.requestId);
    }}
  />
  <div class="px-3 pb-1">
    <DaemonPortalChip compact />
  </div>
  <ChatComposerBar
    mobile
    disabled={connection.offline}
    composerBlocked={chat.composerBlocked}
    onkeydown={handleKeydown}
    onfocus={handleComposerFocus}
    onblur={handleComposerBlur}
    onOpenVoiceSettings={() => layout.openYou("runtime")}
  />
</form>
