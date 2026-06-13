<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import DaemonPortalChip from "$lib/components/chat/DaemonPortalChip.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { createTurnTicket } from "$lib/daemon";
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
    const accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt,
      mode,
      provider: opts.provider,
      model: opts.model,
      responseDepthMode: opts.responseDepthMode,
      stageRouting: opts.stageRouting,
      channelSurface: opts.channelSurface,
    });
    chat.beginTurn(userContent, accepted);
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
    if (!prompt) return;
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
        await submitTurn(prompt, askPrompt, "background");
        return;
      }

      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      await submitTurn(prompt, prompt, mode);
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
      layout.setMobileTab("work");
      const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
      if (pending) void workspace.selectCard(pending.requestId);
    }}
  />
  <div class="px-3 pb-1">
    <DaemonPortalChip compact />
  </div>
  <div class="composer-bar composer-bar-mobile">
    <GrowingTextarea
      bind:value={chat.draft}
      placeholder="Message"
      disabled={connection.offline || chat.composerBlocked}
      maxHeight={144}
      minHeight={34}
      onkeydown={handleKeydown}
      onfocus={handleComposerFocus}
      onblur={handleComposerBlur}
      aria-label="Message"
    />
    <button
      type="submit"
      class="composer-bar-send"
      disabled={connection.offline || chat.composerBlocked || !chat.draft.trim()}
      aria-label="Send message"
      onmousedown={(event) => event.preventDefault()}
    >
      {chat.composerBlocked ? "…" : "↑"}
    </button>
  </div>
</form>
