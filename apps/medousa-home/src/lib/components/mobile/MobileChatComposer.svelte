<script lang="ts">
  import type { createAgentSessionController } from "$lib/chat/agentSessionController.svelte";
  import type { createExternalConversationController } from "$lib/chat/externalConversationController.svelte";
  let { agentSession, externalConversation }: {
    agentSession: ReturnType<typeof createAgentSessionController>;
    externalConversation: ReturnType<typeof createExternalConversationController>;
  } = $props();
  import MobileChatContext from "./MobileChatContext.svelte";
  import { Mic } from "@lucide/svelte";
  import { isTauriIos } from "$lib/platform";
  import { liveVoiceState } from "$lib/liveVoice";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import PeerProposalBar from "$lib/components/chat/PeerProposalBar.svelte";
  import ModeProposalBar from "$lib/components/chat/ModeProposalBar.svelte";
  import AgentPermissionBar from "$lib/components/chat/AgentPermissionBar.svelte";
  import AgentSecretBar from "$lib/components/chat/AgentSecretBar.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import { applyActiveAgentPrompt } from "$lib/utils/activeAgentPrompt";
  import { submitChatTurn } from "$lib/chat/submitTurnController";
  import { isProviderConversationRuntime } from "$lib/utils/sessionAgentRuntime";
  import { composerAttachments } from "$lib/stores/composerAttachments.svelte";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { pendingComposeLaunch } from "$lib/composeLaunch";
  import { layout } from "$lib/runtime/layout.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { getSessionAgentMode, getSessionCodeBinding } from "$lib/daemon";
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

  const providerRuntime = $derived(isProviderConversationRuntime(agentSession.sessionRuntime));
  const blocked = $derived(chat.composerBlocked || runtime.savingControls || externalConversation.busy || agentSession.preparingAgent);

  function scrollToLatest() {
    window.dispatchEvent(new CustomEvent("medousa-chat-scroll-to-bottom", { detail: { force: true } }));
  }

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

  $effect(() => {
    const request = $pendingComposeLaunch;
    if (!request) return;
    pendingComposeLaunch.set(null);
    void (async () => {
      switchMobileTab("chat");
      if (request.action === "new") {
        await chat.newSession();
        window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
        return;
      }
      if (request.action === "notes") {
        switchMobileTab("notes");
        return;
      }
      if (request.action === "calendar") {
        layout.openMore("calendar");
        return;
      }
      if (request.action === "projects") {
        switchMobileTab("home");
        return;
      }
      window.setTimeout(() => {
        window.dispatchEvent(new CustomEvent("medousa-compose-action", { detail: request }));
      });
    })();
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
    await submitChatTurn({
      userContent,
      prompt,
      mode,
      codeProjectSetupAuthorized,
      synchronizeAgentSession: agentSession.synchronizeAgentSession,
      onAgentSessionLost: () => { agentSession.agentConfigOptions = []; },
      scrollToLatest,
    });
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline || blocked || chat.pendingMediaUploading) return;
    if (providerRuntime) {
      const draft = chat.draft;
      const sessionId = chat.sessionId;
      const scope = chat.workshopScopeId;
      if (!draft.trim()) return;
      const attachments = composerAttachments.forHost("chat");
      try {
        await externalConversation.send(draft.trim(), chat.pendingMediaRefs.length > 0 || attachments.skillIds.length > 0 || attachments.toolIds.length > 0);
        if (chat.sessionId === sessionId && chat.workshopScopeId === scope && chat.draft === draft) chat.clearComposerDraft();
        scrollToLatest();
      } catch {
        // The shared controller shows the error and preserves the draft.
      }
      return;
    }
    const basePrompt = ensureVaultSelectionInPrompt(
      chat.draft.trim(),
      chat.vaultNoteContext,
    );
    const prompt = bots.forSession(chat.sessionId)
      ? basePrompt
      : applyActiveAgentPrompt(basePrompt);
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
  {#if !providerRuntime && chat.hasWorkshopHandoff()}
    <p class="mb-1.5 px-1 text-[11px] font-medium text-content-link/90">
      Steering handoff — your next message continues the worker
    </p>
  {/if}
  {#if !providerRuntime && chat.vaultNoteContext}
    <VaultChatContextChip compact class="mb-2" />
  {/if}
  {#if !providerRuntime && chat.streamError}
    <p class="mb-2 px-1 text-xs text-content-error" role="alert">{chat.streamError}</p>
  {/if}
  {#if !providerRuntime}
  <PeerProposalBar mobile sessionId={chat.focusedSessionId} />
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
  {/if}
  <div class="flex min-w-0 items-center justify-between gap-2">
  <div class="min-w-0 flex-1">
    {#if !providerRuntime}<MobileChatContext {agentSession} disabled={connection.offline || blocked}/>{/if}
  </div>
  {#if agentSession.sessionRuntime === "medousa" && isTauriIos() && !$liveVoiceState.active}
    <button
      type="button"
      class="mr-2 mb-1 flex min-h-11 shrink-0 items-center gap-2 rounded-full border border-white/10 bg-white/5 px-3 text-sm text-white disabled:opacity-45"
      disabled={connection.offline || $liveVoiceState.phase === "connecting"}
      aria-label="Start Medousa Live in this conversation"
      onclick={() => window.dispatchEvent(new CustomEvent("medousa-live-start"))}
    >
      <Mic class="size-4" /> Talk live
    </button>
  {/if}
  </div>
  <ChatComposerBar
    mobile
    disabled={connection.offline || externalConversation.busy}
    composerBlocked={blocked}
    agentRuntime={agentSession.sessionRuntime}
    agentConfigOptions={agentSession.agentConfigOptions}
    agentRuntimePending={agentSession.preparingAgent}
    onAgentConfigChange={agentSession.updateAgentConfig}
    externalConversations={externalConversation.conversations}
    externalConversationId={externalConversation.selectedId}
    onExternalConversationChange={externalConversation.select}
    onExternalConversationsRefresh={externalConversation.refresh}
    onfocus={handleComposerFocus}
    onblur={handleComposerBlur}
  />
</form>
