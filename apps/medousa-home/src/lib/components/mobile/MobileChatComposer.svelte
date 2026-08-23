<script lang="ts">
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import ModeProposalBar from "$lib/components/chat/ModeProposalBar.svelte";
  import AgentPermissionBar from "$lib/components/chat/AgentPermissionBar.svelte";
  import AgentSecretBar from "$lib/components/chat/AgentSecretBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import { applyActiveAgentPrompt } from "$lib/utils/activeAgentPrompt";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { createTurnTicket, getSessionAgentMode, getSessionCodeBinding } from "$lib/daemon";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import { hasVisionMediaRefs } from "$lib/types/media";
  import { visionProfileReady } from "$lib/types/inferenceProfiles";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import { setMobileComposerFocus } from "$lib/utils/mobileKeyboardViewport";
  import { ensureVaultSelectionInPrompt } from "$lib/utils/vaultNoteBridge";
  import { activeCodeContext } from "$lib/utils/undertakingWorkspace";

  let composerBlurTimer: ReturnType<typeof setTimeout> | undefined;
  let formEl = $state<HTMLFormElement | null>(null);
  let allowUnboundCoderSend = false;

  $effect(() => {
    const sendToSetup = () => {
      allowUnboundCoderSend = true;
      formEl?.requestSubmit();
    };
    window.addEventListener("medousa-code-project-agent-setup", sendToSetup);
    return () => window.removeEventListener("medousa-code-project-agent-setup", sendToSetup);
  });

  function parseDaemonAskPrompt(value: string): string | null {
    const slash = parseChatSlashInput(value);
    if (slash?.kind === "ask") return slash.prompt;
    return null;
  }

  async function submitTurn(
    userContent: string,
    prompt: string,
    mode: "interactive" | "background",
    codeProjectSetupAuthorized = false,
  ) {
    const opts = buildInteractiveTurnOptions();
    const mediaRefs = [...chat.pendingMediaRefs];
    const voice = voicePresets.turnVoiceFields();
    const codeContext = activeCodeContext(chat.sessionId);
    const accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt,
      mode,
      codeContext,
      codeProjectSetupAuthorized,
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
    chat.beginTurn(
      userContent,
      accepted,
      mediaRefs,
      opts.identityUserId,
    );
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
    if (connection.offline || runtime.savingControls) return;
    const prompt = applyActiveAgentPrompt(
      ensureVaultSelectionInPrompt(chat.draft.trim(), chat.vaultNoteContext),
    );
    const hasAttachments = chat.pendingMediaRefs.length > 0;
    if (!prompt && !hasAttachments) return;
    if (
      hasVisionMediaRefs(chat.pendingMediaRefs) &&
      !visionProfileReady(runtime.inferenceProfiles)
    ) {
      chat.setError(
        "Configure a vision model on the host workshop (Settings → Medousa Agent) before sending images.",
      );
      return;
    }
    if (!allowUnboundCoderSend && !activeCodeContext(chat.sessionId)) {
      const [agentMode, binding] = await Promise.all([
        getSessionAgentMode(chat.sessionId),
        getSessionCodeBinding(chat.sessionId),
      ]);
      if (agentMode.effective_mode === "coder" && !binding.work_id) {
        window.dispatchEvent(new CustomEvent("medousa-open-code-project-chooser"));
        return;
      }
    }
    const codeProjectSetupAuthorized = allowUnboundCoderSend;
    allowUnboundCoderSend = false;
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
        const workId = chat.activeWorkshopWorkId();
        if (!workId) throw new Error("Active workshop generation is missing");
        await steerBoundWorkshop(chat.sessionId, workId, prompt);
        await chat.reloadCurrentSession();
        window.dispatchEvent(
          new CustomEvent("medousa-chat-scroll-to-bottom", {
            detail: { force: true },
          }),
        );
        return;
      }

      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      const display =
        prompt ||
        (hasAttachments ? `[${pendingMediaLabels(chat.pendingMediaRefs)}]` : "");
      await submitTurn(display, prompt, mode, codeProjectSetupAuthorized);
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

<form bind:this={formEl} class="mobile-chat-composer" onsubmit={submit}>
  {#if chat.hasWorkshopHandoff()}
    <p class="mb-1.5 px-1 text-[11px] font-medium text-content-link/90">
      Steering handoff — your next message continues the worker
    </p>
  {/if}
  {#if chat.vaultNoteContext}
    <VaultChatContextChip compact class="mb-2" />
  {/if}
  {#if chat.streamError}
    <p class="mb-2 px-1 text-xs text-content-error" role="alert">{chat.streamError}</p>
  {/if}
  <BudgetApprovalBar
    mobile
    onOpenWork={() => {
      switchMobileTab("home");
      const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
      if (pending) void workspace.selectCard(pending.workCardId);
    }}
  />
  <ModeProposalBar
    mobile
    sessionId={chat.focusedSessionId}
  />
  <AgentPermissionBar mobile />
  <AgentSecretBar mobile />
  <AgentBrowserPanel mobile />
  <div class="mb-1 flex items-center px-1">
    <ChatAgentModePicker
      sessionId={chat.focusedSessionId}
      disabled={connection.offline || chat.composerBlocked || runtime.savingControls}
    />
    <div class="ml-1 min-w-0"><UndertakingContextChip chatOnly /></div>
  </div>
  <ChatComposerBar
    mobile
    disabled={connection.offline}
    composerBlocked={chat.composerBlocked || runtime.savingControls}
    onkeydown={handleKeydown}
    onfocus={handleComposerFocus}
    onblur={handleComposerBlur}
  />
</form>
