<script lang="ts">
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import MobileComposerTurnSettings from "$lib/components/mobile/MobileComposerTurnSettings.svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { createTurnTicket } from "$lib/daemon";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import { hasVisionMediaRefs } from "$lib/types/media";
  import { visionProfileReady } from "$lib/types/inferenceProfiles";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import { setMobileComposerFocus } from "$lib/utils/mobileKeyboardViewport";
  import { ensureVaultSelectionInPrompt } from "$lib/utils/vaultNoteBridge";

  let composerBlurTimer: ReturnType<typeof setTimeout> | undefined;

  const controlsDisabled = $derived(
    connection.offline || chat.composerBlocked || runtime.savingControls,
  );

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
    const voice = voicePresets.turnVoiceFields();
    const accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt,
      mode,
      provider: opts.provider,
      model: opts.model,
      responseDepthMode: opts.responseDepthMode,
      reasoningEffort: opts.reasoningEffort,
      stageRouting: opts.stageRouting,
      channelSurface: opts.channelSurface,
      mediaRefs,
      voicePresetId: voice.voicePresetId,
      voiceAppendix: voice.voiceAppendix,
      identityUserId: opts.identityUserId,
    });
    chat.beginTurn(userContent, accepted, mediaRefs);
    chat.clearPendingMedia();
    window.dispatchEvent(
      new CustomEvent("medousa-chat-scroll-to-bottom", { detail: { force: true } }),
    );
    await chat.startTurnStream(
      accepted.turn_id,
      accepted.session_id,
      accepted.stream_url,
    );
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline) return;
    const prompt = ensureVaultSelectionInPrompt(
      chat.draft.trim(),
      chat.vaultNoteContext,
    );
    const hasAttachments = chat.pendingMediaRefs.length > 0;
    if (!prompt && !hasAttachments) return;
    if (
      hasVisionMediaRefs(chat.pendingMediaRefs) &&
      !visionProfileReady(runtime.inferenceProfiles)
    ) {
      chat.setError(
        "Configure a vision model on the host workshop (Settings → Models) before sending images.",
      );
      return;
    }
    haptic("medium");

    const askPrompt = parseDaemonAskPrompt(prompt);
    const slash = parseChatSlashInput(prompt);
    chat.clearComposerDraft();
    chat.clearVaultNoteContext();

    try {
      if (slash && slash.kind !== "ask") {
        await runSlashCommand(slash);
        return;
      }

      if (askPrompt) {
        await submitTurn(prompt || pendingMediaLabels(chat.pendingMediaRefs), askPrompt, "background");
        return;
      }

      if (chat.hasWorkshopHandoff()) {
        const { steerBoundWorkshop } = await import("$lib/daemon");
        await steerBoundWorkshop(chat.sessionId, prompt);
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
    chat.flushDraftPersist();
    composerBlurTimer = setTimeout(() => {
      setMobileComposerFocus(false);
      composerBlurTimer = undefined;
    }, 150);
  }
</script>

<form class="mobile-chat-composer" onsubmit={submit}>
  <div class="mobile-composer-pills">
    <WorkshopSwitcherCompact />
    <ProfileSwitcherCompact />
    {#if settings.showChatModelPicker}
      <MobileComposerTurnSettings disabled={controlsDisabled} />
    {/if}
  </div>
  {#if chat.vaultNoteContext}
    <VaultChatContextChip compact class="mb-2" />
  {/if}
  {#if chat.streamError}
    <p class="mb-2 px-1 text-xs text-error-400" role="alert">{chat.streamError}</p>
  {/if}
  <BudgetApprovalBar
    mobile
    onOpenWork={() => {
      switchMobileTab("home");
      const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
      if (pending) void workspace.selectCard(pending.workCardId);
    }}
  />
  <AgentBrowserPanel mobile />
  <ChatComposerBar
    mobile
    disabled={connection.offline}
    composerBlocked={chat.composerBlocked}
    onkeydown={handleKeydown}
    onfocus={handleComposerFocus}
    onblur={handleComposerBlur}
  />
</form>
