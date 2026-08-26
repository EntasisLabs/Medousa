<script lang="ts">
  import { tick, untrack } from "svelte";
  import { ExternalLink, LoaderCircle } from "@lucide/svelte";
  import ChatAsyncToolsHint from "$lib/components/chat/ChatAsyncToolsHint.svelte";
  import ChatChangeReceipt from "$lib/components/chat/ChatChangeReceipt.svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import ChatDerivationNotice from "$lib/components/chat/ChatDerivationNotice.svelte";
  import ChatPresenceDock from "$lib/components/chat/ChatPresenceDock.svelte";
  import ChatScrollChrome from "$lib/components/chat/ChatScrollChrome.svelte";
  import { createAgentSessionController } from "$lib/chat/agentSessionController.svelte";
  import { submitChatTurn } from "$lib/chat/submitTurnController";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import ComposerSkillPills from "$lib/components/chat/ComposerSkillPills.svelte";
  import ComposerSkillSlashMenu from "$lib/components/chat/ComposerSkillSlashMenu.svelte";
  import ComposerDraftsControl from "$lib/components/chat/ComposerDraftsControl.svelte";
  import ComposerTurnControls from "$lib/components/chat/ComposerTurnControls.svelte";
  import AgentSessionControls from "$lib/components/chat/AgentSessionControls.svelte";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import ModeProposalBar from "$lib/components/chat/ModeProposalBar.svelte";
  import AgentPermissionBar from "$lib/components/chat/AgentPermissionBar.svelte";
  import AgentSecretBar from "$lib/components/chat/AgentSecretBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import ScriptChatContextChip from "$lib/components/grapheme/ScriptChatContextChip.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { activeCodeContext } from "$lib/utils/undertakingWorkspace";
  import { haptic } from "$lib/haptics";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { composerAttachments } from "$lib/stores/composerAttachments.svelte";
  import { activeAgent } from "$lib/stores/activeAgent.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    getSessionAgentMode,
    getSessionCodeBinding,
    steerBoundWorkshop,
  } from "$lib/daemon";
  import {
    formatSessionLabel,
    presenceRoomTitle,
    presenceSubline,
  } from "$lib/utils/formatSession";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import { applyActiveAgentPrompt } from "$lib/utils/activeAgentPrompt";
  import {
    ensureVaultSelectionInPrompt,
    vaultContextHasSelection,
  } from "$lib/utils/vaultNoteBridge";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { groupAskThreads, isChatLaneMessage } from "$lib/utils/askThreads";
  import { openWorkAsks } from "$lib/utils/workChromeEvents";
  import WorkerTranscriptPanel from "$lib/components/chat/WorkerTranscriptPanel.svelte";
  import { subagentRowMap, subagentRowsForSession } from "$lib/utils/subagentRows";
  import {
    saveChatTurnToVault,
    showChatTurnSaveFeedback,
  } from "$lib/utils/saveChatTurnToVault";
  import type { ChatMessage } from "$lib/types/chat";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import {
    buildAskJobRequest,
  } from "$lib/utils/askPrompt";
  import {
    buildComposerSlashItems,
    composerSlashToken,
    stripComposerSlashToken,
    type ComposerSlashItem,
  } from "$lib/utils/composerSkillSlash";
  import {
    placeComposerSlashMenuAnchor,
    type SlashMenuAnchor,
  } from "$lib/utils/slashMenuPlacement";
  import OfflineChatGate from "$lib/components/chat/OfflineChatGate.svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
  import { isTauri, showChatPopout } from "$lib/window";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
    /** Already hosted in the dedicated chat window. */
    popout?: boolean;
    workshop?: boolean;
    /** Soft sticky-note bottom sheet — quieter empty/composer chrome. */
    workshopSticky?: boolean;
    scriptWorkbench?: boolean;
    onOpenContext?: () => void;
    onOpenConnection?: () => void;
  }

  let {
    visible,
    mobile = false,
    embedded = false,
    popout = false,
    workshop = false,
    workshopSticky = false,
    scriptWorkbench = false,
    onOpenContext,
    onOpenConnection,
  }: Props = $props();

  const agentSession = createAgentSessionController();
  let presenceComposerCentered = $state(false);
  let runPresenceDockBlurp: () => Promise<void> | void = $state(() => {});
  let scrollToLatest: (force?: boolean, behavior?: ScrollBehavior) => void = $state(
    (_force?: boolean, _behavior?: ScrollBehavior) => {},
  );
  let scheduleChatNavigationMeasure: () => void = $state(() => {});
  let resetScrollSession: () => void = $state(() => {});
  let scrollEl: HTMLDivElement | undefined = $state();
  let atBottom = $state(true);
  let activeChatTurnId = $state<string | null>(null);
  let chatScrolling = $state(false);
  let cardDetailOpen = $state(false);
  let cardDetail = $state<CardDetailPayload | null>(null);
  let workerTranscriptWorkId = $state<string | null>(null);

  function openCardDetail(detail: CardDetailPayload) {
    cardDetail = detail;
    cardDetailOpen = true;
  }

  function closeCardDetail() {
    cardDetailOpen = false;
    cardDetail = null;
  }

  function openWorkerTranscript(workId: string) {
    const trimmed = workId.trim();
    if (!trimmed) return;
    workerTranscriptWorkId = trimmed;
  }

  function closeWorkerTranscript() {
    workerTranscriptWorkId = null;
  }

  function stopWorker(workId: string) {
    const trimmed = workId.trim();
    if (!trimmed) return;
    void import("$lib/daemon").then(({ cancelWorkspaceCard }) => {
      void cancelWorkspaceCard(trimmed);
    });
  }

  function prefillComposerFromChip(label: string) {
    const trimmed = label.trim();
    if (!trimmed) return;
    chat.draft = trimmed;
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }

  const scrollPinThresholdPx = $derived(mobile ? 24 : 96);

  /** Stable principal — ignores temporary session swaps during background SSE. */
  const panelSessionId = $derived(chat.focusedSessionId);
  const chatCodeProject = $derived.by(() => {
    const active = undertakings.active;
    if (!active?.boundChatSessionIds.includes(panelSessionId)) return null;
    return active;
  });
  const panelMessages = $derived(chat.messagesFor(panelSessionId));
  /**
   * Worker-lane turns stay in the principal thread: they carry the sub-agent's
   * synthesis prose, and their position is where the beat belongs chronologically.
   */
  const chatMessages = $derived(
    panelMessages.filter(
      (message) => isChatLaneMessage(message) || message.lane === "worker",
    ),
  );
  const derivationSource = $derived.by(() =>
    chatMessages.find((message) => message.transcript?.source)?.transcript?.source ?? null,
  );
  const derivationSourceLabel = $derived.by(() => {
    if (!derivationSource) return "Source conversation";
    const session = chat.sessions.find(
      (entry) => entry.session_id === derivationSource.sessionId,
    );
    return session
      ? formatSessionLabel(session)
      : `Conversation ${derivationSource.sessionId.slice(-8)}`;
  });
  const subagentRows = $derived(subagentRowsForSession(panelSessionId));
  const subagentRowsByWorkId = $derived(subagentRowMap(panelSessionId));
  const activeSubagentCount = $derived(subagentRows.filter((row) => row.streaming).length);
  const askThreads = $derived(groupAskThreads(panelMessages));
  const showInlineComposer = $derived(!mobile || (embedded && scriptWorkbench));
  const useMobileChatLayout = $derived(mobile);
  /** The centered new-chat state exists only after an explicit New action. */
  const showChatEmptyState = $derived(
    chat.sessionPristine &&
      chatMessages.length === 0 &&
      subagentRows.length === 0,
  );

  /** Don't treat "history still loading" as empty Presence — that centers the dock on cold start. */
  const historyPending = $derived(
    chat.historyLoadingFor(panelSessionId) && panelMessages.length === 0,
  );

  /** Presence — the quiet, centered landing for a genuinely empty main chat. */
  const showPresenceEmpty = $derived(
    showChatEmptyState &&
      !historyPending &&
      !workshop &&
      !scriptWorkbench &&
      !embedded,
  );
  const presenceAsk = $derived(presenceSubline());

  const chatAttachments = composerAttachments.chat;
  let draftCursor = $state(0);
  let slashHighlight = $state(0);
  let slashAnchor = $state<SlashMenuAnchor | null>(null);
  let composerFormEl = $state<HTMLFormElement | null>(null);
  let composerTextareaEl = $state<HTMLTextAreaElement | null>(null);
  let allowUnboundCoderSend = false;

  $effect(() => {
    if (mobile) return;
    const sendToSetup = () => {
      allowUnboundCoderSend = true;
      composerFormEl?.requestSubmit();
    };
    window.addEventListener("medousa-code-project-agent-setup", sendToSetup);
    return () => window.removeEventListener("medousa-code-project-agent-setup", sendToSetup);
  });

  const slashToken = $derived(composerSlashToken(chat.draft, draftCursor));
  const slashItems = $derived(
    slashToken
      ? buildComposerSlashItems({
          filter: slashToken.filter,
          manuscripts: catalog.manuscripts,
          capabilities: catalog.capabilities,
          attachedSkillIds: chatAttachments.skillIds,
          attachedToolIds: chatAttachments.toolIds,
          includeCommands: true,
        })
      : [],
  );
  /** Token start the operator dismissed with Escape — draft keeps the /token. */
  let slashDismissedStart = $state<number | null>(null);
  const slashMenuOpen = $derived(
    Boolean(
      slashToken &&
        slashItems.length > 0 &&
        slashDismissedStart !== slashToken.start,
    ),
  );

  function dismissSlashMenu() {
    slashDismissedStart = slashToken?.start ?? null;
  }

  $effect(() => {
    void slashItems.length;
    slashHighlight = 0;
    if (!slashToken) slashDismissedStart = null;
    if (!slashMenuOpen || !composerTextareaEl) {
      slashAnchor = null;
      return;
    }
    const rect = composerTextareaEl.getBoundingClientRect();
    slashAnchor = placeComposerSlashMenuAnchor({
      top: rect.top,
      bottom: rect.bottom,
      left: rect.left,
    });
  });

  $effect(() => {
    if (slashMenuOpen && catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  function applyChatSlashItem(item: ComposerSlashItem) {
    const token = slashToken;
    if (!token) return;
    slashDismissedStart = null;
    if (item.kind === "skill") {
      chatAttachments.attachSkill(item.id);
      if (chatAttachments.skillIds.length === 1) {
        activeAgent.setActive(item.id);
      }
      const next = stripComposerSlashToken(chat.draft, token, "");
      chat.draft = next.value;
      draftCursor = next.cursor;
    } else if (item.kind === "tool") {
      chatAttachments.attachTool(item.id);
      const next = stripComposerSlashToken(chat.draft, token, "");
      chat.draft = next.value;
      draftCursor = next.cursor;
    } else {
      const next = stripComposerSlashToken(chat.draft, token, item.insert);
      chat.draft = next.value;
      draftCursor = next.cursor;
    }
    void tick().then(() => {
      composerTextareaEl?.focus();
      composerTextareaEl?.setSelectionRange(draftCursor, draftCursor);
    });
  }

  function handlePromoteToFlow(ref: ToolHistorySliceRef) {
    flowDraft.queuePromotion([ref]);
    automationsNav.openSection("flows");
    layout.navigateDesktop("automations", { bump: true });
    if (mobile) layout.openMore("automations");
  }

  async function openChatCodeReview(path?: string, line?: number) {
    const project = chatCodeProject;
    if (!project) return;
    if (path) {
      await lmeWorkspace.openCodeFile(project.workId, path, { line: line ?? 1 });
      return;
    }
    if (project.humanPhase === "review") {
      await lmeWorkspace.openCodeReview(project.workId, `Review · ${project.title}`);
      return;
    }
    await lmeWorkspace.openCodeWorkspace(project.workId, project.title);
  }

  function requestChatCodeRevision(prompt?: string) {
    const project = chatCodeProject;
    if (!project) return;
    chat.draft = prompt ?? `Revise the current changes in ${project.title}. `;
    void tick().then(() => {
      composerTextareaEl?.focus();
      composerTextareaEl?.setSelectionRange(chat.draft.length, chat.draft.length);
      window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
    });
  }

  async function handleSaveToVault(assistant: ChatMessage, user?: ChatMessage | null) {
    const result = await saveChatTurnToVault({
      assistant,
      user: user ?? null,
      sessionId: panelSessionId,
    });
    showChatTurnSaveFeedback(result);
  }
  const sessionLabel = $derived.by(() => {
    // Presence empty: always the time-of-day room title — don't keep a stale preview.
    if (showPresenceEmpty) return presenceRoomTitle();
    const session = chat.sessions.find((entry) => entry.session_id === panelSessionId);
    return session
      ? formatSessionLabel(session)
      : formatSessionLabel({
          session_id: panelSessionId,
          preview: "",
          turns: 0,
          verification_runs: 0,
        });
  });
  /** Most recently active other session — surfaced as "continue where you left off". */
  const continueSession = $derived.by(() => {
    const others = chat.sessions.filter(
      (session) => session.session_id !== panelSessionId && session.turns > 0,
    );
    if (others.length === 0) return null;
    return [...others].sort((a, b) => {
      const at = a.last_timestamp ? Date.parse(a.last_timestamp) : 0;
      const bt = b.last_timestamp ? Date.parse(b.last_timestamp) : 0;
      return bt - at;
    })[0];
  });
  const streamingMessage = $derived(
    panelMessages.find((message) => message.streaming && message.role === "assistant"),
  );
  const phaseLine = $derived.by(() => {
    if (!streamingMessage) return null;
    const status = visibleChatStatusLine(
      streamingMessage.statusLine,
      settings.showEngineDetailsInChat,
    );
    if (status) return status;
    if (streamingMessage.phase) return formatTurnPhase(streamingMessage.phase);
    if (streamingMessage.tools?.length) {
      return streamingMessage.tools.map((tool) => formatToolName(tool)).join(" · ");
    }
    return "Working…";
  });

  const mobileChatTitle = $derived.by(() => {
    if (!mobile) return "Medousa";
    if (chat.backgroundActivity > 0) {
      return chat.backgroundActivity === 1
        ? "Working in background"
        : `${chat.backgroundActivity} turns active`;
    }
    return "Medousa";
  });

  const mobileChatSubtitle = $derived.by(() => {
    if (!mobile) return sessionLabel;
    if (chat.liveStreamActive && phaseLine) return phaseLine;
    if (chat.liveStreamActive) return "Thinking…";
    if (chat.backgroundActivity > 0) return "Background work · see Work";
    if (showChatEmptyState) return presenceAsk;
    if (chat.historyLoadingFor(panelSessionId) && panelMessages.length === 0) {
      return "Opening thread…";
    }
    const last = [...panelMessages].reverse().find((message) => message.content.trim());
    if (last?.content) {
      const line = last.content.trim().split("\n")[0];
      if (/^done\s*[—–-]\s*vault/i.test(line)) {
        return "Saved to Vault";
      }
      return line.length > 56 ? `${line.slice(0, 55)}…` : line;
    }
    return "Ready when you are";
  });

  const showScrollFab = $derived(
    !atBottom &&
      (chatMessages.length > 0 || subagentRows.length > 0),
  );

  const latestUserTurn = $derived.by(() => {
    for (let index = chatMessages.length - 1; index >= 0; index -= 1) {
      const message = chatMessages[index];
      if (message?.role === "user" && message.content.trim()) return message;
    }
    return null;
  });
  const latestUserPreview = $derived.by(() => {
    const firstLine = latestUserTurn?.content.trim().split("\n")[0] ?? "";
    return firstLine.length > 120 ? `${firstLine.slice(0, 119)}…` : firstLine;
  });
  const chatTurnItems = $derived(
    chatMessages
      .filter((message) => message.role === "user" && message.content.trim())
      .map((message) => {
        const firstLine = message.content.trim().split("\n")[0];
        return {
          id: message.id,
          text: firstLine.length > 120 ? `${firstLine.slice(0, 119)}…` : firstLine,
          depth: 2,
        };
      }),
  );
  const showChatTurnRail = $derived(
    !embedded && !useMobileChatLayout && chatTurnItems.length > 1,
  );
  const showCurrentTurnAnchor = $derived(Boolean(latestUserTurn && latestUserPreview));

  $effect(() => {
    void panelSessionId;
    resetScrollSession();
  });

  $effect(() => {
    if (!scrollEl) return;
    void chatMessages
      .map((message) =>
        [
          message.content.length,
          message.segments?.length ?? -1,
          message.toolRuns?.map((run) => `${run.runId}:${run.status}`).join(",") ?? "",
        ].join(":"),
      )
      .join("\0");
    void subagentRows.map((row) => row.statusLine).join("\0");
    void chat.hasTurnActivity;
    scrollToLatest(false);
    void tick().then(scheduleChatNavigationMeasure);
  });

  $effect(() => {
    if (!visible) return;
    const onComposerFocus = () => {
      if (atBottom) scrollToLatest(true);
    };
    const onScrollRequest = (event: Event) => {
      const force = (event as CustomEvent<{ force?: boolean }>).detail?.force ?? false;
      scrollToLatest(force, force ? "smooth" : "auto");
    };
    window.addEventListener("medousa-chat-composer-focus", onComposerFocus);
    window.addEventListener("medousa-chat-scroll-to-bottom", onScrollRequest);
    return () => {
      window.removeEventListener("medousa-chat-composer-focus", onComposerFocus);
      window.removeEventListener("medousa-chat-scroll-to-bottom", onScrollRequest);
    };
  });

  $effect(() => {
    if (!visible || !chat.historyNotice) return;
    const notice = chat.historyNotice;
    const timer = setTimeout(() => {
      if (chat.historyNotice === notice) {
        chat.clearHistoryNotice();
      }
    }, 4000);
    return () => clearTimeout(timer);
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
      onAgentSessionLost: () => {
        agentSession.agentConfigOptions = [];
      },
      scrollToLatest: () => scrollToLatest(true),
    });
  }

  $effect(() => {
    const { sessionId, runtimeChoice } = agentSession.syncFromFocusedSession();
    if (runtimeChoice !== "medousa") {
      // The lifecycle queue updates its busy counter synchronously. Keep that
      // counter outside this bootstrap effect's dependency graph.
      void untrack(() => agentSession.synchronizeAgentSession(sessionId, runtimeChoice)).catch(
        () => {
          // First send retries and surfaces connection/provider errors.
        },
      );
    }
  });

  $effect(() => {
    window.addEventListener(
      "medousa-code-project-binding-changed",
      agentSession.onCodeProjectBindingChanged as EventListener,
    );
    return () =>
      window.removeEventListener(
        "medousa-code-project-binding-changed",
        agentSession.onCodeProjectBindingChanged as EventListener,
      );
  });

  type FailedSend = {
    display: string;
    prompt: string;
    mode: "interactive" | "background";
    codeProjectSetupAuthorized: boolean;
  };
  /** Last turn that threw — kept so the error banner can offer Retry. */
  let lastFailedSend = $state<FailedSend | null>(null);

  async function retryLastSend() {
    const payload = lastFailedSend;
    if (!payload) return;
    lastFailedSend = null;
    chat.clearStreamError(panelSessionId);
    try {
      await submitTurn(
        payload.display,
        payload.prompt,
        payload.mode,
        payload.codeProjectSetupAuthorized,
      );
    } catch (err) {
      lastFailedSend = payload;
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }

  function dismissStreamError() {
    lastFailedSend = null;
    chat.clearStreamError(panelSessionId);
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline || runtime.savingControls) return;
    const scopeForSend = chat.vaultNoteContext;
    const prompt = applyActiveAgentPrompt(
      ensureVaultSelectionInPrompt(chat.draft.trim(), scopeForSend),
    );
    const hasAttachments = chat.pendingMediaRefs.length > 0;
    if (!prompt && !hasAttachments) return;
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
    if (mobile) haptic("medium");

    const askPrompt = parseDaemonAskPrompt(prompt);
    const slash = parseChatSlashInput(prompt);
    let pendingSend: FailedSend | null = null;
    lastFailedSend = null;
    chat.clearComposerDraft();
    if (!chat.pinVaultNoteContext) {
      chat.clearVaultNoteContext();
    }

    try {
      if (slash && slash.kind !== "ask") {
        await runSlashCommand(slash);
        return;
      }

      if (presenceComposerCentered) void runPresenceDockBlurp();

      if (askPrompt) {
        await workspace.submitAsk({
          ...buildAskJobRequest(
            askPrompt,
            chatAttachments.skillIds,
            chatAttachments.toolIds,
          ),
          modelHint: runtime.model,
        });
        chatAttachments.clear();
        chat.historyNotice = "Ask queued — watch Work for progress.";
        return;
      }

      if (chat.hasWorkshopHandoff()) {
        const workId = chat.activeWorkshopWorkId();
        if (!workId) throw new Error("Active workshop generation is missing");
        await steerBoundWorkshop(chat.sessionId, workId, prompt);
        await chat.reloadCurrentSession();
        scrollToLatest(true);
        return;
      }

      const primarySkill = chatAttachments.primarySkillId;
      if (primarySkill) {
        activeAgent.setActive(primarySkill);
      }

      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      const display =
        prompt ||
        (hasAttachments ? `[${pendingMediaLabels(chat.pendingMediaRefs)}]` : "");
      pendingSend = { display, prompt, mode, codeProjectSetupAuthorized };
      await submitTurn(display, prompt, mode, codeProjectSetupAuthorized);
    } catch (err) {
      lastFailedSend = pendingSend;
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (slashMenuOpen && slashItems.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        slashHighlight = (slashHighlight + 1) % slashItems.length;
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        slashHighlight =
          (slashHighlight - 1 + slashItems.length) % slashItems.length;
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        const item = slashItems[slashHighlight];
        if (item) applyChatSlashItem(item);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        dismissSlashMenu();
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit(event);
    }
  }

  async function resumeSession(sessionId: string) {
    await chat.switchSession(sessionId);
  }

  async function openDerivationSource() {
    const sourceSessionId = derivationSource?.sessionId;
    if (!sourceSessionId) return;
    await chat.switchSession(sourceSessionId);
    const { shellTabs } = await import("$lib/stores/shellTabs.svelte");
    shellTabs.openChat(sourceSessionId, { activate: true });
  }

  function continueWhereLeftOff() {
    if (!continueSession) return;
    void resumeSession(continueSession.session_id);
  }

  async function sendStarterPrompt(prompt: string) {
    if (connection.offline || chat.composerBlocked) return;
    if (mobile) haptic("light");
    const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
    const fullPrompt = ensureVaultSelectionInPrompt(prompt, chat.vaultNoteContext);
    try {
      lastFailedSend = null;
      await submitTurn(fullPrompt, fullPrompt, mode);
    } catch (err) {
      lastFailedSend = {
        display: fullPrompt,
        prompt: fullPrompt,
        mode,
        codeProjectSetupAuthorized: false,
      };
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }

  /** A Liquid scene interaction (action_row / button) starting a new turn. */
  function submitChatIntent(text: string) {
    const trimmed = text.trim();
    if (!trimmed) return;
    void sendStarterPrompt(trimmed);
  }
</script>

<section
  class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible
    ? ''
    : 'hidden'} {embedded && useMobileChatLayout
    ? 'script-workbench-chat-mobile-root'
    : embedded
      ? `vault-workshop-chat-panel${workshopSticky ? ' vault-workshop-chat-panel--sticky' : ''}`
      : useMobileChatLayout
        ? 'mobile-chat-panel'
        : 'chat-pane'}"
>
  {#if !embedded}
  <header class="{mobile ? 'mobile-chat-header' : 'workshop-header'}">
    <div class="flex w-full min-w-0 items-center gap-2">
      {#if !mobile}
        <ShellSidebarExpandButton label="Show sessions" />
        <button
          type="button"
          class="min-w-0 text-left"
          onclick={() => {
            if (!layout.shellSidebarExpanded) {
              layout.openShellSidebarView("chat");
            }
          }}
        >
          <h1 class="truncate text-sm font-semibold text-surface-50">{sessionLabel}</h1>
        </button>
        <UndertakingContextChip chatOnly header />
      {:else}
        <div class="min-w-0 py-1">
          <div class="flex min-w-0 items-center gap-2">
            <h1 class="truncate text-sm font-semibold text-surface-50">
              {mobileChatTitle}
            </h1>
            <UndertakingContextChip chatOnly header />
          </div>
          <p class="text-content-tertiary truncate text-[11px]">{mobileChatSubtitle}</p>
        </div>
        {#if chat.hasTurnActivity}
          <span
            class="badge shrink-0 variant-soft-primary text-[10px] font-medium normal-case"
            title={chat.liveStreamActive
              ? "Live turn streaming"
              : `${chat.backgroundActivity} background turn(s)`}
          >
            {#if chat.liveStreamActive}
              Live
            {:else}
              {chat.backgroundActivity} active
            {/if}
          </span>
        {/if}
      {/if}
      {#if !mobile && !popout && isTauri()}
        <button
          type="button"
          class="chat-view-popout"
          title="Pop out chat"
          aria-label="Pop out chat"
          onclick={() => void showChatPopout()}
        >
          <ExternalLink size={14} strokeWidth={1.8} />
        </button>
      {/if}
    </div>
    {#if chat.streamErrorFor(panelSessionId)}
      <div class="mt-1 flex flex-wrap items-baseline gap-2">
        <p class="text-content-error min-w-0 flex-1 text-[11px]" role="alert">
          {chat.streamErrorFor(panelSessionId)}
        </p>
        {#if lastFailedSend}
          <button
            type="button"
            class="chat-stream-error-action"
            onclick={() => void retryLastSend()}
          >
            Retry
          </button>
        {/if}
        <button type="button" class="chat-stream-error-action" onclick={dismissStreamError}>
          Dismiss
        </button>
      </div>
    {:else if !mobile && chat.historyLoadingFor(panelSessionId) && panelMessages.length === 0}
      <p class="mt-1 text-[11px] text-content-tertiary">Loading conversation…</p>
    {/if}
  </header>
  {/if}

  {#if mobile && chat.liveStreamActive && phaseLine}
    <div class="mobile-chat-phase" aria-live="polite">
      <span
        class="inline-block h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-primary-400"
        aria-hidden="true"
      ></span>
      <span class="min-w-0 truncate">{phaseLine}</span>
    </div>
  {/if}

  {#if userProfiles.switchNotice && visible}
    <div
      class="chat-restore-toast {mobile ? 'chat-restore-toast-mobile' : ''}"
      role="status"
    >
      <span class="min-w-0">{userProfiles.switchNotice}</span>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="chat-restore-toast-dismiss text-content-link"
          onclick={() => {
            void chat.newSession();
            userProfiles.dismissSwitchNotice();
          }}
        >
          New chat
        </button>
        <button
          type="button"
          class="chat-restore-toast-dismiss"
          aria-label="Dismiss"
          onclick={() => userProfiles.dismissSwitchNotice()}
        >
          ✕
        </button>
      </div>
    </div>
  {/if}

  {#if chat.historyNotice && visible}
    <div
      class="chat-restore-toast {mobile ? 'chat-restore-toast-mobile' : ''}"
      role="status"
    >
      <span class="min-w-0 truncate">{chat.historyNotice}</span>
      <button
        type="button"
        class="chat-restore-toast-dismiss"
        aria-label="Dismiss"
        onclick={() => chat.clearHistoryNotice()}
      >
        ✕
      </button>
    </div>
  {/if}

  {#if chat.askHandoffNotice && visible && !embedded}
    <div
      class="chat-restore-toast {mobile ? 'chat-restore-toast-mobile' : ''}"
      role="status"
    >
      <span class="min-w-0 truncate">{chat.askHandoffNotice}</span>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="chat-restore-toast-dismiss text-content-link"
          onclick={() => {
            chat.clearAskHandoffNotice();
            openWorkAsks();
          }}
        >
          Open in Work
        </button>
        <button
          type="button"
          class="chat-restore-toast-dismiss"
          aria-label="Dismiss"
          onclick={() => chat.clearAskHandoffNotice()}
        >
          ✕
        </button>
      </div>
    </div>
  {/if}

  <div class="relative flex min-h-0 flex-1 flex-col">
  <ChatScrollChrome
    {mobile}
    pinThresholdPx={scrollPinThresholdPx}
    showFab={showScrollFab && visible}
    showTurnRail={showChatTurnRail}
    {showCurrentTurnAnchor}
    {latestUserPreview}
    latestUserTurnId={latestUserTurn?.id ?? null}
    {chatTurnItems}
    bind:activeChatTurnId
    bind:chatScrolling
    bind:scrollEl
    bind:scrollToLatest
    bind:scheduleChatNavigationMeasure
    bind:resetForSession={resetScrollSession}
    onAtBottomChange={(value) => (atBottom = value)}
    bodyClass={embedded && !useMobileChatLayout
      ? "vault-workshop-chat-body"
      : useMobileChatLayout
        ? "mobile-chat-body"
        : "chat-body"}
    scrollClass="{embedded && !useMobileChatLayout
      ? 'vault-workshop-chat-scroll space-y-3'
      : useMobileChatLayout
        ? 'mobile-chat-scroll space-y-3'
        : 'chat-scroll space-y-4'} {showPresenceEmpty ? 'chat-scroll--presence' : ''}"
  >
      <ChatAsyncToolsHint {mobile} />
      {#if askThreads.length > 0 && !embedded && mobile}
        <button
          type="button"
          class="mobile-chat-rail-chip"
          onclick={() => openWorkAsks()}
        >
          <span>
            {askThreads.length} background ask{askThreads.length === 1 ? "" : "s"} in Work
          </span>
          <span class="text-content-quiet">→</span>
        </button>
      {/if}

      {#if chatMessages.length > 0}
        {#if derivationSource}
          <ChatDerivationNotice
            sourceLabel={derivationSourceLabel}
            onOpenSource={openDerivationSource}
          />
        {/if}
        <ChatMessageList
          messages={chatMessages}
          sessionId={panelSessionId}
          {mobile}
          navigation
          scrollRoot={scrollEl}
          onPromoteToFlow={handlePromoteToFlow}
          onSubmitIntent={submitChatIntent}
          onSaveToVault={handleSaveToVault}
          onOpenCardDetail={openCardDetail}
          subagentRows={subagentRowsByWorkId}
          onOpenSubagent={openWorkerTranscript}
          onStopSubagent={stopWorker}
        />
      {:else if showChatEmptyState}
        {#if scriptWorkbench && chat.scriptWorkbenchContext}
        <div
          class="flex min-h-[120px] flex-col justify-center {embedded ? 'px-3 py-2' : mobile ? 'px-1 pb-4' : 'px-2'}"
        >
          <p class="text-sm text-content-tertiary">Ask about this script — fixes, modules, or next steps.</p>
          <div class="mt-3 flex flex-wrap gap-2">
            {#each ["Explain this script", "Fix compile errors", "Suggest a module to use"] as prompt (prompt)}
              <button
                type="button"
                class="rounded-full border border-surface-500/40 bg-surface-950/50 px-3 py-1.5 text-xs text-surface-200 transition hover:border-primary-400/50 hover:text-surface-50"
                disabled={connection.offline || chat.composerBlocked}
                onclick={() => void sendStarterPrompt(prompt)}
              >
                {prompt}
              </button>
            {/each}
          </div>
        </div>
        {:else if workshop && chat.vaultNoteContext}
        <div
          class="flex min-h-[120px] flex-col justify-center {embedded ? 'px-3 py-2' : mobile ? 'px-1 pb-4' : 'px-2'}"
        >
          {#if workshopSticky}
            <p class="text-content-quiet px-1 text-[12px] leading-relaxed">
              {vaultContextHasSelection(chat.vaultNoteContext)
                ? "Ask about this passage…"
                : "Ask about this note…"}
            </p>
          {:else}
            <p class="text-sm text-content-tertiary">
              {vaultContextHasSelection(chat.vaultNoteContext)
                ? "Work this passage with Medousa — edit, clarify, or next steps."
                : "Ask about this note — links, edits, or next steps."}
            </p>
            <div class="mt-3 flex flex-wrap gap-2">
              {#each vaultContextHasSelection(chat.vaultNoteContext)
                ? ["Suggest an edit", "Clarify this", "Expand this"]
                : ["What links here?", "Summarize this note", "Suggest edits"] as prompt (prompt)}
                <button
                  type="button"
                  class="rounded-full border border-surface-500/40 bg-surface-950/50 px-3 py-1.5 text-xs text-surface-200 transition hover:border-primary-400/50 hover:text-surface-50"
                  disabled={connection.offline || chat.composerBlocked}
                  onclick={() => void sendStarterPrompt(prompt)}
                >
                  {prompt}
                </button>
              {/each}
            </div>
          {/if}
        </div>
        {/if}
      {:else if chat.historyLoadingFor(panelSessionId) && panelMessages.length === 0 && !mobile}
      <div class="flex min-h-[200px] items-center justify-center">
        <LoaderCircle size={22} class="animate-spin text-content-quiet/80" aria-label="Loading" />
      </div>
      {/if}
      {#if chatCodeProject && !embedded}
        <ChatChangeReceipt
          workId={chatCodeProject.workId}
          projectTitle={chatCodeProject.title}
          phase={chatCodeProject.humanPhase}
          review={undertakings.review?.work_id === chatCodeProject.workId
            ? undertakings.review
            : null}
          eventRevision={undertakings.eventRevision}
          onOpenCode={openChatCodeReview}
          onRequestRevision={requestChatCodeRevision}
          onReviewChanged={() => undertakings.select(chatCodeProject.workId)}
        />
      {/if}
    </ChatScrollChrome>

  <ChatPresenceDock
    bind:composerCentered={presenceComposerCentered}
    bind:runBlurp={runPresenceDockBlurp}
    showEmpty={showPresenceEmpty}
    {showInlineComposer}
    {presenceAsk}
    showContinue={Boolean(continueSession)}
    onContinue={continueWhereLeftOff}
  >
    {#if !embedded && !presenceComposerCentered}
      <BudgetApprovalBar
        onOpenWork={() => {
          workspace.workView = "hub";
          const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
          if (pending) void workspace.selectCard(pending.workCardId);
        }}
      />
      <ModeProposalBar
        sessionId={panelSessionId}
      />
      <AgentPermissionBar />
      <AgentSecretBar />
      {#if activeSubagentCount > 0}
        <button
          type="button"
          class="chat-subagent-pill"
          onclick={() => {
            const running = subagentRows.find((row) => row.streaming);
            if (running) openWorkerTranscript(running.workId);
          }}
        >
          <span class="chat-subagent-pill-dot" aria-hidden="true"></span>
          {activeSubagentCount} subagent{activeSubagentCount === 1 ? "" : "s"} working
        </button>
      {/if}
      <AgentBrowserPanel />
    {/if}
    <form
      bind:this={composerFormEl}
      class="{embedded
        ? useMobileChatLayout
          ? 'mobile-chat-composer script-workbench-chat-composer'
          : workshopSticky
            ? 'vault-workshop-chat-composer vault-workshop-chat-composer--sticky'
            : 'vault-workshop-chat-composer'
        : 'chat-composer'}"
      onsubmit={submit}
    >
      {#if chat.scriptWorkbenchContext}
        <ScriptChatContextChip compact={workshop || scriptWorkbench} class={embedded ? "mb-2" : "mx-4 mb-2"} />
      {:else if chat.vaultNoteContext}
        <VaultChatContextChip
          compact={workshop}
          whisper={workshopSticky}
          class={workshop ? "mb-1.5" : "mx-4 mb-2"}
        />
      {/if}
      <ComposerSkillPills
        host="chat"
        disabled={connection.offline || chat.composerBlocked}
        class={workshop ? "mb-1.5" : "mx-4 mb-2"}
      />
      {#if chat.hasWorkshopHandoff()}
        <p
          class="{workshop ? 'mb-1.5' : 'mx-4 mb-1.5'} text-[11px] font-medium text-content-link/90"
        >
          Steering handoff — your next message continues the worker
        </p>
      {/if}
      <ChatComposerBar
        mobile={workshop || useMobileChatLayout}
        disabled={connection.offline}
        composerBlocked={chat.composerBlocked}
        modelPickerEnabled
        agentRuntime={agentSession.sessionRuntime}
        agentConfigOptions={agentSession.agentConfigOptions}
        agentRuntimePending={agentSession.preparingAgent}
        onAgentRuntimeChange={agentSession.onRuntimeChange}
        onAgentConfigChange={agentSession.updateAgentConfig}
        bind:element={composerTextareaEl}
        onkeydown={handleKeydown}
        onCursorChange={(cursor) => (draftCursor = cursor)}
      />
      {#if !workshop && !embedded}
        <div class="chat-runtime-under">
          <ChatAgentModePicker
            sessionId={panelSessionId}
            disabled={connection.offline || chat.composerBlocked || agentSession.preparingAgent}
          />
          <ComposerTurnControls
            disabled={connection.offline || chat.composerBlocked}
            showNativeControls={agentSession.sessionRuntime === "medousa"}
          />
          {#if agentSession.sessionRuntime !== "medousa"}
            <AgentSessionControls
              options={agentSession.agentConfigOptions}
              includeModel={false}
              disabled={connection.offline || chat.composerBlocked || agentSession.preparingAgent}
              onChange={agentSession.updateAgentConfig}
            />
          {/if}
          <ComposerDraftsControl
            disabled={connection.offline || chat.composerBlocked}
            mode={agentSession.sessionRuntime}
            model={`${runtime.provider}:${runtime.model}`}
          />
        </div>
      {/if}
      <ComposerSkillSlashMenu
        open={slashMenuOpen}
        items={slashItems}
        anchor={slashAnchor}
        highlightIndex={slashHighlight}
        onSelect={applyChatSlashItem}
        onClose={dismissSlashMenu}
        onHighlight={(index) => (slashHighlight = index)}
      />
    </form>
  </ChatPresenceDock>
  </div>

  {#if visible && connection.offline}
    <OfflineChatGate {mobile} {onOpenConnection} />
  {/if}

  <LiquidCardDetailSheet
    open={cardDetailOpen}
    detail={cardDetail}
    onClose={closeCardDetail}
    onChipSelect={prefillComposerFromChip}
  />

  <WorkerTranscriptPanel
    workId={workerTranscriptWorkId}
    onClose={closeWorkerTranscript}
  />
</section>

<style>
  .chat-view-popout {
    display: inline-flex;
    width: 1.6rem;
    height: 1.6rem;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    margin-left: auto;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    transition: background-color 120ms ease, color 120ms ease;
  }

  .chat-view-popout:hover {
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--color-surface-100));
  }

  .chat-stream-error-action {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
  }

  .chat-stream-error-action:hover {
    color: rgb(var(--color-surface-100));
  }

  /* Ambient count only — the beats in-thread carry the detail. */
  .chat-subagent-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    align-self: flex-start;
    margin: 0 1rem 0.375rem;
    border-radius: 999px;
    padding: 0.125rem 0.5rem;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-tertiary));
    transition: color 120ms ease;
  }

  .chat-subagent-pill:hover {
    color: rgb(var(--theme-text-secondary));
  }

  .chat-subagent-pill-dot {
    width: 0.25rem;
    height: 0.25rem;
    border-radius: 999px;
    background: rgb(var(--color-primary-400));
    animation: subagent-pulse 1.8s ease-in-out infinite;
  }

  @keyframes subagent-pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }
</style>
