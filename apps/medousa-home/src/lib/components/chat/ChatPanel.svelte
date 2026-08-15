<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { ArrowDown, LoaderCircle } from "@lucide/svelte";
  import ChatAsyncToolsHint from "$lib/components/chat/ChatAsyncToolsHint.svelte";
  import ChatChangeReceipt from "$lib/components/chat/ChatChangeReceipt.svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import MarkdownHeadingOutline from "$lib/components/ui/MarkdownHeadingOutline.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import ComposerSkillPills from "$lib/components/chat/ComposerSkillPills.svelte";
  import ComposerSkillSlashMenu from "$lib/components/chat/ComposerSkillSlashMenu.svelte";
  import ComposerTurnControls from "$lib/components/chat/ComposerTurnControls.svelte";
  import AgentSessionControls from "$lib/components/chat/AgentSessionControls.svelte";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import ModeProposalBar from "$lib/components/chat/ModeProposalBar.svelte";
  import AgentPermissionBar from "$lib/components/chat/AgentPermissionBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import ScriptChatContextChip from "$lib/components/grapheme/ScriptChatContextChip.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { activeCodeContext } from "$lib/utils/undertakingWorkspace";
  import { planAgentWorkspace } from "$lib/utils/agentWorkspacePlan";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { composerAttachments } from "$lib/stores/composerAttachments.svelte";
  import { activeAgent } from "$lib/stores/activeAgent.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    cancelAgentSession,
    createAgentSession,
    createTurnTicket,
    getSessionAgentMode,
    getSessionCodeBinding,
    promptAgentSession,
    setAgentSessionConfigOption,
    steerBoundWorkshop,
    type AgentSessionConfigOption,
  } from "$lib/daemon";
  import {
    agentSessionStreamUrl,
    clearSessionAgentSessionId,
    getSessionAgentRuntime,
    getSessionAgentSessionId,
    getSessionAgentConfigOptions,
    getSessionAgentWorkId,
    setSessionAgentRuntime,
    setSessionAgentSessionId,
    setSessionAgentConfigOptions,
    setSessionAgentWorkId,
    type ChatAgentRuntime,
  } from "$lib/utils/sessionAgentRuntime";
  import type { TurnTicketResponse } from "$lib/types/session";
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
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

  interface Props {
    visible: boolean;
    mobile?: boolean;
    embedded?: boolean;
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
    workshop = false,
    workshopSticky = false,
    scriptWorkbench = false,
    onOpenContext,
    onOpenConnection,
  }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();
  let atBottom = $state(true);
  let activeChatTurnId = $state<string | null>(null);
  let chatScrolling = $state(false);
  let pinLatestUserTurn = $state(false);
  let chatNavigationFrame = 0;
  let chatScrollEndTimer: ReturnType<typeof setTimeout> | undefined;
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

  let presenceDockMode = $state<"center" | "docking" | "docked">("docked");
  let presenceDockEl = $state<HTMLDivElement | undefined>(undefined);
  let presenceEmptyEl = $state<HTMLDivElement | undefined>(undefined);
  let presenceAskEl = $state<HTMLParagraphElement | undefined>(undefined);
  let presenceContinueEl = $state<HTMLButtonElement | undefined>(undefined);
  let presenceBlurpToken = 0;
  let presenceDockLocked = $state(false);
  /** translateY offset that parks the bottom-anchored dock on the 2/3 seam. */
  let presenceCenterOffset = $state(0);
  let presenceCenterPlaced = $state(false);

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

  const presenceComposerCentered = $derived(
    showPresenceEmpty &&
      showInlineComposer &&
      (presenceDockMode === "center" || presenceDockMode === "docking"),
  );

  function clearPresenceDockInlineStyles() {
    const el = presenceDockEl;
    if (!el) return;
    el.getAnimations().forEach((animation) => animation.cancel());
    el.style.transition = "";
    el.style.transform = "";
    el.style.transformOrigin = "";
    el.style.willChange = "";
    el.style.backfaceVisibility = "";
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
  const showCurrentTurnAnchor = $derived(
    Boolean(latestUserTurn && latestUserPreview && pinLatestUserTurn),
  );

  $effect(() => {
    void panelSessionId;
    atBottom = true;
    activeChatTurnId = null;
    pinLatestUserTurn = false;
  });

  $effect(() => {
    if (!scrollEl) return;
    void chatMessages.map((message) => message.content).join("\0");
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

  /** Presence dock — float to center for a fresh landing, dock back down once busy. */
  $effect(() => {
    if (
      showPresenceEmpty &&
      showInlineComposer &&
      !presenceDockLocked &&
      presenceDockMode === "docked"
    ) {
      presenceDockMode = "center";
    }
  });

  $effect(() => {
    if (showPresenceEmpty && showInlineComposer) return;
    // History arrived / session not empty — hard-dock and scrub center transform.
    // Cold start used to leave translateY inline after hydrate → composer mid-pane.
    presenceBlurpToken += 1;
    presenceDockLocked = false;
    presenceCenterOffset = 0;
    presenceCenterPlaced = false;
    presenceDockMode = "docked";
    clearPresenceDockInlineStyles();
  });

  /**
   * Presence seams (thirds of the chat panel):
   * - 1/3 line → center of the ask title (continue sits under it)
   * - 2/3 line → center of the composer dock
   */
  async function placePresenceSeams() {
    presenceCenterPlaced = false;
    await tick();
    await tick();
    const dock = presenceDockEl;
    const parent = dock?.parentElement ?? presenceEmptyEl?.parentElement;
    if (!parent || presenceDockMode !== "center") return;

    const parentRect = parent.getBoundingClientRect();
    const seam2 = parentRect.top + (parentRect.height * 2) / 3;

    // Ask title: center on the 1/3 seam.
    // Continue: slightly above the midpoint between title (1/3) and input (2/3).
    const empty = presenceEmptyEl;
    const ask = presenceAskEl;
    const cont = presenceContinueEl;
    if (empty && ask) {
      empty.style.left = "50%";
      empty.style.transform = "translateX(-50%)";
      const askHeight = ask.offsetHeight;
      const emptyTop = parentRect.height / 3 - askHeight / 2;
      empty.style.top = `${Math.max(0, emptyTop)}px`;

      if (cont) {
        const titleCenterY = parentRect.height / 3;
        const inputCenterY = (parentRect.height * 2) / 3;
        const midY = (titleCenterY + inputCenterY) / 2;
        const continueCenterY = midY - parentRect.height * 0.08;
        const contHeight = cont.offsetHeight || 18;
        const margin = continueCenterY - emptyTop - askHeight - contHeight / 2;
        cont.style.marginTop = `${Math.max(10, margin)}px`;
      }
    }

    // Composer dock: bottom-anchored, translate so its center hits the 2/3 seam.
    if (dock) {
      dock.style.transition = "none";
      dock.style.transform = "translate3d(0, 0, 0)";
      void dock.offsetHeight;

      const dockRect = dock.getBoundingClientRect();
      const dockCenter = dockRect.top + dockRect.height / 2;
      const offset = seam2 - dockCenter;
      presenceCenterOffset = offset;
      dock.style.transform = `translate3d(0, ${offset}px, 0)`;
    }
    presenceCenterPlaced = true;
  }

  $effect(() => {
    if (presenceDockMode !== "center" || presenceDockLocked) return;
    void presenceDockEl;
    void presenceEmptyEl;
    void presenceAskEl;
    void presenceContinueEl;
    void placePresenceSeams();
  });

  $effect(() => {
    if (presenceDockMode !== "center" || presenceDockLocked) return;
    const parent = presenceDockEl?.parentElement ?? presenceEmptyEl?.parentElement;
    if (!parent || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      void placePresenceSeams();
    });
    ro.observe(parent);
    if (presenceDockEl) ro.observe(presenceDockEl);
    return () => ro.disconnect();
  });

  function prefersReducedMotion(): boolean {
    return (
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true
    );
  }

  /**
   * Slime-drop: one continuous WAAPI deform (stretch in flight → soft splat → settle).
   * Ask fades in parallel — no staged shrink/move/expand.
   */
  async function runPresenceDockBlurp() {
    presenceDockLocked = true;
    const token = ++presenceBlurpToken;
    const el = presenceDockEl;
    const y = presenceCenterOffset;

    if (!el || prefersReducedMotion()) {
      presenceDockMode = "docked";
      presenceCenterOffset = 0;
      presenceCenterPlaced = false;
      clearPresenceDockInlineStyles();
      return;
    }

    el.getAnimations().forEach((animation) => animation.cancel());
    presenceDockMode = "docking";
    el.style.transition = "none";
    el.style.transformOrigin = "50% 50%";
    el.style.willChange = "transform";
    el.style.backfaceVisibility = "hidden";
    el.style.transform = `translate3d(0, ${y}px, 0) scale3d(1, 1, 1)`;

    /**
     * Dense samples of one continuous hourglass curve (not hand-keyed corners).
     * Same duration — smoother because each step is tiny + C1-ish easing.
     */
    const smootherstep = (t: number) =>
      t * t * t * (t * (t * 6 - 15) + 10);
    const mix = (a: number, b: number, t: number) => a + (b - a) * t;

    const STEPS = 20;
    const NECK = 0.5;
    const FALL_START = 0.16; // pinch first, then fall
    const NECK_AT = 0.58; // narrowest just past mid-drop

    const keyframes: Keyframe[] = [];
    for (let i = 0; i <= STEPS; i += 1) {
      const t = i / STEPS;

      // Y: hold, then smooth fall (no per-segment easing kinks)
      const fallT =
        t <= FALL_START ? 0 : smootherstep((t - FALL_START) / (1 - FALL_START));
      const yPos = y * (1 - fallT);

      // Width: hourglass — shrink to neck, then bloom
      let scaleX: number;
      if (t <= NECK_AT) {
        scaleX = mix(1, NECK, smootherstep(t / NECK_AT));
      } else {
        scaleX = mix(NECK, 1, smootherstep((t - NECK_AT) / (1 - NECK_AT)));
      }

      // Height: slight stretch in the neck, ease back — keeps mass feeling continuous
      const pinch = 1 - scaleX; // 0 at bulbs, max at neck
      const scaleY = 1 + pinch * 0.35;

      keyframes.push({
        transform: `translate3d(0, ${yPos}px, 0) scale3d(${scaleX}, ${scaleY}, 1)`,
        offset: t,
      });
    }

    const drop = el.animate(keyframes, {
      duration: 1080,
      easing: "linear",
      fill: "forwards",
    });

    try {
      await drop.finished;
    } catch {
      /* aborted */
    }
    if (token !== presenceBlurpToken) return;

    // Hold the final identity frame, then clear — avoids a cancel() snap.
    el.style.transform = "translate3d(0, 0, 0) scale3d(1, 1, 1)";
    drop.cancel();
    await tick();
    if (token !== presenceBlurpToken) return;

    el.style.transition = "";
    el.style.transform = "";
    el.style.transformOrigin = "";
    el.style.willChange = "";
    el.style.backfaceVisibility = "";
    presenceCenterOffset = 0;
    presenceDockMode = "docked";
  }

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
    const runtime = getSessionAgentRuntime(chat.sessionId);
    if (runtime !== "medousa" && mode === "interactive" && !codeProjectSetupAuthorized) {
      const prepared = await synchronizeAgentSession(chat.sessionId, runtime, {
        openChooserWhenMissing: true,
      });
      if (!prepared) throw new Error("Choose a project before starting a coding agent.");
      const { agentSessionId, streamUrl, streamReady, acceptedAt } = prepared;

      const ticket: TurnTicketResponse = {
        turn_id: agentSessionId,
        session_id: chat.sessionId,
        mode: "interactive",
        phase: "accepted" as TurnTicketResponse["phase"],
        accepted_at_utc: acceptedAt,
        stream_url: streamUrl || agentSessionStreamUrl(agentSessionId),
        stream_ready: streamReady,
      };
      chat.beginTurn(
        userContent,
        ticket,
        [],
        userProfiles.activeProfileId,
      );
      chat.clearPendingMedia();
      scrollToLatest(true);
      await chat.startTurnStream(
        ticket.turn_id,
        ticket.session_id,
        ticket.stream_url,
      );
      try {
        await promptAgentSession(agentSessionId, prompt, activeCodeContext(chat.sessionId));
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        if (/unknown agent session|not found|404/i.test(message)) {
          clearSessionAgentSessionId(chat.sessionId);
          setSessionAgentConfigOptions(chat.sessionId, []);
          agentConfigOptions = [];
        }
        throw err;
      }
      return;
    }

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
      opts.identityUserId ?? userProfiles.activeProfileId,
    );
    chat.clearPendingMedia();
    scrollToLatest(true);
    await chat.startTurnStream(
      accepted.turn_id,
      accepted.session_id,
      accepted.stream_url,
    );
  }

  let sessionRuntime = $state<ChatAgentRuntime>(
    getSessionAgentRuntime(chat.sessionId),
  );
  let agentConfigOptions = $state<AgentSessionConfigOption[]>(
    getSessionAgentConfigOptions(chat.sessionId) as AgentSessionConfigOption[],
  );
  let agentLifecyclePending = $state(0);
  const preparingAgent = $derived(agentLifecyclePending > 0);
  let agentLifecycleQueue: Promise<void> = Promise.resolve();

  type PreparedAgentSession = {
    agentSessionId: string;
    streamUrl: string;
    streamReady: boolean;
    acceptedAt: string;
  };

  function queueAgentLifecycle<T>(operation: () => Promise<T>): Promise<T> {
    agentLifecyclePending += 1;
    const queued = agentLifecycleQueue.catch(() => undefined).then(operation);
    agentLifecycleQueue = queued.then(
      () => undefined,
      () => undefined,
    );
    return queued.finally(() => {
      agentLifecyclePending = Math.max(0, agentLifecyclePending - 1);
    });
  }

  async function cancelKnownAgent(sessionId: string, agentSessionId: string) {
    try {
      await cancelAgentSession(agentSessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (!/unknown agent session|not found|404/i.test(message)) throw err;
    }
    clearSessionAgentSessionId(sessionId);
    setSessionAgentConfigOptions(sessionId, []);
    if (chat.sessionId === sessionId) agentConfigOptions = [];
  }

  function synchronizeAgentSession(
    sessionId: string,
    runtimeChoice: Exclude<ChatAgentRuntime, "medousa">,
    options?: { openChooserWhenMissing?: boolean; stopWhenUnbound?: boolean },
  ): Promise<PreparedAgentSession | null> {
    return queueAgentLifecycle(async () => {
      const [binding, mode] = await Promise.all([
        getSessionCodeBinding(sessionId),
        getSessionAgentMode(sessionId),
      ]);
      if (getSessionAgentRuntime(sessionId) !== runtimeChoice) return null;

      const bindingWorkId = binding.work_id?.trim() || null;
      const currentAgentId = getSessionAgentSessionId(sessionId);
      const action =
        options?.stopWhenUnbound && !bindingWorkId
          ? currentAgentId
            ? "stop"
            : "keep"
          : planAgentWorkspace({
              runtime: runtimeChoice,
              mode: mode.effective_mode,
              bindingWorkId,
              agentSessionId: currentAgentId,
              agentWorkId: getSessionAgentWorkId(sessionId),
            });

      if ((action === "stop" || action === "restart") && currentAgentId) {
        await cancelKnownAgent(sessionId, currentAgentId);
      }
      if (action === "stop" || action === "wait_for_project") {
        if (options?.openChooserWhenMissing && chat.sessionId === sessionId) {
          window.dispatchEvent(new CustomEvent("medousa-open-code-project-chooser"));
        }
        return null;
      }

      const retainedAgentId = getSessionAgentSessionId(sessionId);
      if (action === "keep" && retainedAgentId) {
        return {
          agentSessionId: retainedAgentId,
          streamUrl: agentSessionStreamUrl(retainedAgentId),
          streamReady: true,
          acceptedAt: new Date().toISOString(),
        };
      }

      const accepted = await createAgentSession({
        session_id: sessionId,
        runtime: runtimeChoice,
        // The daemon resolves this work id to the governed worktree and
        // overrides any client cwd. Plain general chat deliberately uses null.
        work_id: bindingWorkId,
      });
      const latestBinding = await getSessionCodeBinding(sessionId);
      const latestWorkId = latestBinding.work_id?.trim() || null;
      if (
        getSessionAgentRuntime(sessionId) !== runtimeChoice ||
        latestWorkId !== bindingWorkId
      ) {
        await cancelAgentSession(accepted.agent_session_id).catch(() => undefined);
        return null;
      }

      setSessionAgentSessionId(sessionId, accepted.agent_session_id);
      setSessionAgentWorkId(sessionId, bindingWorkId);
      const configOptions = accepted.config_options ?? [];
      setSessionAgentConfigOptions(sessionId, configOptions);
      if (chat.sessionId === sessionId) agentConfigOptions = configOptions;
      return {
        agentSessionId: accepted.agent_session_id,
        streamUrl: accepted.stream_url,
        streamReady: accepted.stream_ready,
        acceptedAt: accepted.accepted_at_utc ?? new Date().toISOString(),
      };
    });
  }

  $effect(() => {
    const sessionId = chat.sessionId;
    const runtimeChoice = getSessionAgentRuntime(sessionId);
    sessionRuntime = runtimeChoice;
    agentConfigOptions = getSessionAgentConfigOptions(
      sessionId,
    ) as AgentSessionConfigOption[];
    if (runtimeChoice !== "medousa") {
      // The lifecycle queue updates its busy counter synchronously. Keep that
      // counter outside this bootstrap effect's dependency graph.
      void untrack(() => synchronizeAgentSession(sessionId, runtimeChoice)).catch(
        () => {
          // First send retries and surfaces connection/provider errors.
        },
      );
    }
  });

  function onRuntimeChange(value: ChatAgentRuntime) {
    const sessionId = chat.sessionId;
    const previousRuntime = getSessionAgentRuntime(sessionId);
    const previousId = getSessionAgentSessionId(sessionId);
    const previousWorkId = getSessionAgentWorkId(sessionId);
    const previousConfigOptions = getSessionAgentConfigOptions(sessionId);
    sessionRuntime = value;
    setSessionAgentRuntime(sessionId, value);
    agentConfigOptions = [];
    void (async () => {
      if (previousId) {
        try {
          await queueAgentLifecycle(() => cancelKnownAgent(sessionId, previousId));
        } catch (err) {
          // A failed provider cancellation must not strand an unreachable ACP
          // process. Restore the prior local handle so Stop/retry still works.
          if (getSessionAgentRuntime(sessionId) === value) {
            setSessionAgentRuntime(sessionId, previousRuntime);
            setSessionAgentSessionId(sessionId, previousId);
            if (previousWorkId !== undefined) {
              setSessionAgentWorkId(sessionId, previousWorkId);
            }
            setSessionAgentConfigOptions(sessionId, previousConfigOptions);
            if (chat.sessionId === sessionId) {
              sessionRuntime = previousRuntime;
              agentConfigOptions = previousConfigOptions as AgentSessionConfigOption[];
              chat.setError(err instanceof Error ? err.message : String(err));
            }
          }
          return;
        }
      }
      if (value !== "medousa") {
        await synchronizeAgentSession(sessionId, value, { openChooserWhenMissing: true }).catch(
          () => {
            // Sending the first message retries and surfaces provider errors.
          },
        );
      }
    })();
  }

  $effect(() => {
    const onBindingChanged = (
      event: Event & { detail?: { sessionId?: string; workId?: string | null } },
    ) => {
      const sessionId = event.detail?.sessionId?.trim();
      if (!sessionId || sessionId !== chat.sessionId) return;
      const runtimeChoice = getSessionAgentRuntime(sessionId);
      if (runtimeChoice === "medousa") return;
      void synchronizeAgentSession(sessionId, runtimeChoice, {
        stopWhenUnbound: !event.detail?.workId,
      }).catch((err) => {
        chat.setError(err instanceof Error ? err.message : String(err));
      });
    };
    window.addEventListener(
      "medousa-code-project-binding-changed",
      onBindingChanged as EventListener,
    );
    return () =>
      window.removeEventListener(
        "medousa-code-project-binding-changed",
        onBindingChanged as EventListener,
      );
  });

  async function updateAgentConfig(configId: string, value: unknown) {
    const agentSessionId = getSessionAgentSessionId(chat.sessionId);
    if (!agentSessionId) return;
    const response = await setAgentSessionConfigOption(agentSessionId, configId, value);
    agentConfigOptions = response.config_options;
    setSessionAgentConfigOptions(chat.sessionId, agentConfigOptions);
  }

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

      if (presenceComposerCentered && presenceDockMode === "center") {
        void runPresenceDockBlurp();
      }

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

  function measureChatNavigation() {
    chatNavigationFrame = 0;
    if (!scrollEl) {
      activeChatTurnId = null;
      pinLatestUserTurn = false;
      return;
    }

    const rootRect = scrollEl.getBoundingClientRect();
    const turns = [
      ...scrollEl.querySelectorAll<HTMLElement>("[data-chat-turn-user-id]"),
    ];
    if (turns.length === 0) {
      activeChatTurnId = null;
      pinLatestUserTurn = false;
      return;
    }

    const threshold = rootRect.top + 64;
    let activeId = turns[0]?.dataset.chatTurnUserId ?? null;
    for (const turn of turns) {
      if (turn.getBoundingClientRect().top <= threshold) {
        activeId = turn.dataset.chatTurnUserId ?? activeId;
      } else {
        break;
      }
    }
    activeChatTurnId = activeId;

    const latestId = latestUserTurn?.id;
    const latestTurn = latestId
      ? turns.find((turn) => turn.dataset.chatTurnUserId === latestId)
      : undefined;
    if (!latestTurn) {
      pinLatestUserTurn = false;
      return;
    }
    const latestRect = latestTurn.getBoundingClientRect();
    const responseIsLong = latestRect.height >= Math.max(280, scrollEl.clientHeight * 0.8);
    const promptHasLeftTop = latestRect.top < rootRect.top + 8;
    const responseStillVisible = latestRect.bottom > rootRect.top + 96;
    pinLatestUserTurn = responseIsLong && promptHasLeftTop && responseStillVisible;
  }

  function scheduleChatNavigationMeasure() {
    if (chatNavigationFrame) return;
    chatNavigationFrame = requestAnimationFrame(measureChatNavigation);
  }

  function onScroll() {
    if (!scrollEl) return;
    const distanceFromBottom =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    atBottom = distanceFromBottom <= scrollPinThresholdPx;
    chatScrolling = true;
    if (chatScrollEndTimer) clearTimeout(chatScrollEndTimer);
    chatScrollEndTimer = setTimeout(() => {
      chatScrolling = false;
      chatScrollEndTimer = undefined;
    }, 160);
    scheduleChatNavigationMeasure();
  }

  function scrollToLatest(force = false, behavior: ScrollBehavior = "auto") {
    if (!scrollEl) return;
    if (!force && !atBottom) return;
    requestAnimationFrame(() => {
      if (!scrollEl) return;
      if (!force && !atBottom) return;
      scrollEl.scrollTo({ top: scrollEl.scrollHeight, behavior });
      atBottom = true;
    });
  }

  function scrollToBottomFromFab() {
    if (mobile) haptic("light");
    scrollToLatest(true, "smooth");
  }

  function scrollToCurrentTurn() {
    const id = latestUserTurn?.id;
    if (id) scrollToChatTurn(id);
  }

  function scrollToChatTurn(id: string) {
    if (!scrollEl) return;
    const target = [...scrollEl.querySelectorAll<HTMLElement>("[data-chat-turn-user-id]")]
      .find((element) => element.dataset.chatTurnUserId === id);
    if (!target) return;
    const rootRect = scrollEl.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    scrollEl.scrollTo({
      top: Math.max(0, scrollEl.scrollTop + targetRect.top - rootRect.top - 12),
      behavior: "smooth",
    });
  }

  onDestroy(() => {
    if (chatNavigationFrame) cancelAnimationFrame(chatNavigationFrame);
    if (chatScrollEndTimer) clearTimeout(chatScrollEndTimer);
  });

  async function resumeSession(sessionId: string) {
    await chat.switchSession(sessionId);
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
    <div class="flex min-w-0 items-center gap-2">
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

  <div class="chat-panel-main">
    {#if showCurrentTurnAnchor}
      <button
        type="button"
        class="chat-current-turn-anchor"
        aria-label="Show your latest message"
        onclick={scrollToCurrentTurn}
      >
        <span class="chat-current-turn-anchor-label">You</span>
        <span class="chat-current-turn-anchor-preview">{latestUserPreview}</span>
      </button>
    {/if}
  <div
    class="{embedded && !useMobileChatLayout
      ? 'vault-workshop-chat-body'
      : useMobileChatLayout
        ? 'mobile-chat-body'
        : 'chat-body'}"
  >
    <div
      bind:this={scrollEl}
      onscroll={onScroll}
      class="{embedded && !useMobileChatLayout
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
    </div>
    {#if !useMobileChatLayout && !presenceComposerCentered}
      <div class="chat-scroll-fade" aria-hidden="true"></div>
    {/if}
    {#if showChatTurnRail}
      <MarkdownHeadingOutline
        items={chatTurnItems}
        activeId={activeChatTurnId}
        scrolling={chatScrolling}
        mode="rail"
        label="Conversation turns"
        onSelect={scrollToChatTurn}
      />
    {/if}
  </div>

  {#if showPresenceEmpty && (presenceDockMode === "center" || presenceDockMode === "docking")}
    <div
      bind:this={presenceEmptyEl}
      class="chat-presence-empty {presenceDockMode === 'docking'
        ? 'chat-presence-empty--exiting'
        : ''} {presenceCenterPlaced || presenceDockMode === 'docking'
        ? 'chat-presence-empty--placed'
        : ''}"
    >
      <p bind:this={presenceAskEl} class="chat-presence-ask">{presenceAsk}</p>
      {#if continueSession}
        <button
          bind:this={presenceContinueEl}
          type="button"
          class="chat-presence-continue"
          onclick={() => void continueWhereLeftOff()}
        >
          Continue where we left off
        </button>
      {/if}
    </div>
  {/if}

  {#if showInlineComposer}
  <div
    bind:this={presenceDockEl}
    class="chat-presence-dock chat-presence-dock--{presenceDockMode}"
    class:chat-presence-dock--placed={presenceCenterPlaced ||
      presenceDockMode === "docking" ||
      presenceDockMode === "docked"}
  >
    {#if !embedded}
      {#if presenceDockMode === "docked"}
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
      {/if}
      {#if presenceDockMode === "docked"}
        <AgentBrowserPanel />
      {/if}
    {/if}
    <form
      bind:this={composerFormEl}
      class="{embedded
        ? useMobileChatLayout
          ? 'mobile-chat-composer script-workbench-chat-composer'
          : workshopSticky
            ? 'vault-workshop-chat-composer vault-workshop-chat-composer--sticky'
            : 'vault-workshop-chat-composer'
        : 'chat-composer'} {presenceComposerCentered ? 'chat-composer--presence-center' : ''}"
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
        agentRuntime={sessionRuntime}
        {agentConfigOptions}
        agentRuntimePending={preparingAgent}
        onAgentRuntimeChange={onRuntimeChange}
        onAgentConfigChange={updateAgentConfig}
        bind:element={composerTextareaEl}
        onkeydown={handleKeydown}
        onCursorChange={(cursor) => (draftCursor = cursor)}
      />
      {#if !workshop && !embedded}
        <div class="chat-runtime-under">
          <ChatAgentModePicker
            sessionId={panelSessionId}
            disabled={connection.offline || chat.composerBlocked || preparingAgent}
          />
          <ComposerTurnControls
            disabled={connection.offline || chat.composerBlocked}
            showNativeControls={sessionRuntime === "medousa"}
          />
          {#if sessionRuntime !== "medousa"}
            <AgentSessionControls
              options={agentConfigOptions}
              includeModel={false}
              disabled={connection.offline || chat.composerBlocked || preparingAgent}
              onChange={updateAgentConfig}
            />
          {/if}
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
  </div>
  {/if}
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

  {#if showScrollFab && visible}
    <button
      type="button"
      class="chat-scroll-fab"
      aria-label="Scroll to latest message"
      onclick={scrollToBottomFromFab}
    >
      <ArrowDown size={22} strokeWidth={2} />
    </button>
  {/if}
</section>

<style>
  .chat-panel-main {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .chat-current-turn-anchor {
    position: absolute;
    top: 0.5rem;
    right: 2.4rem;
    z-index: 4;
    display: flex;
    min-width: 0;
    width: min(32rem, calc(100% - 4.8rem));
    align-items: center;
    gap: 0.55rem;
    padding: 0.5rem 0.7rem;
    border: 1px solid rgb(var(--color-surface-400) / 0.18);
    border-radius: 0.75rem;
    background: rgb(var(--color-surface-900) / 0.94);
    color: rgb(var(--color-surface-200));
    text-align: left;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.3);
    backdrop-filter: blur(12px);
    cursor: pointer;
  }

  .chat-current-turn-anchor:hover,
  .chat-current-turn-anchor:focus-visible {
    border-color: rgb(var(--color-surface-300) / 0.32);
    background: rgb(var(--color-surface-800) / 0.95);
    outline: none;
  }

  .chat-current-turn-anchor-label {
    flex-shrink: 0;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.68rem;
    font-weight: 650;
    white-space: nowrap;
  }

  .chat-current-turn-anchor-preview {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    font-size: 0.75rem;
    line-height: 1.35;
  }

  @media (max-width: 640px) {
    .chat-current-turn-anchor {
      right: 0.75rem;
      width: calc(100% - 1.5rem);
    }
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

  /*
   * Center + docking stay bottom-anchored; visual center is translateY only.
   * That avoids FLIP layout thrash (the jump-up / jump-down you saw).
   */
  .chat-presence-dock--center,
  .chat-presence-dock--docking {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 6;
    display: flex;
    width: 100%;
    flex-direction: column;
    align-items: stretch;
    /* Same geometry as docked — only translateY moves. No width/padding handoff jump. */
    padding: 0;
  }

  .chat-presence-dock--center:not(.chat-presence-dock--placed) {
    visibility: hidden;
  }

  .chat-presence-dock--docked {
    position: relative;
    z-index: 10;
    display: flex;
    width: 100%;
    flex-shrink: 0;
    flex-direction: column;
    align-items: stretch;
    padding: 0;
  }

  .chat-composer--presence-center {
    width: 100%;
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

  .chat-presence-empty {
    position: absolute;
    left: 50%;
    top: 0;
    /* Below the centered composer dock (6) so the model panel can rise over it. */
    z-index: 5;
    display: flex;
    width: max-content;
    max-width: calc(100% - 2rem);
    flex-direction: column;
    align-items: center;
    gap: 0;
    text-align: center;
    transform: translateX(-50%);
    pointer-events: auto;
  }

  .chat-presence-empty:not(.chat-presence-empty--placed):not(.chat-presence-empty--exiting) {
    visibility: hidden;
  }

  .chat-presence-ask {
    margin: 0;
    flex-shrink: 0;
    font-size: clamp(1.4rem, 2.6vw, 1.75rem);
    font-weight: 600;
    line-height: 1.3;
    letter-spacing: -0.02em;
    white-space: nowrap;
    color: rgb(var(--color-surface-50));
  }

  .chat-presence-continue {
    border: 0;
    background: transparent;
    font-size: 0.8125rem;
    color: rgb(var(--theme-text-tertiary));
    text-decoration: underline;
    text-decoration-color: rgb(var(--color-surface-500) / 0.5);
    text-underline-offset: 0.18em;
    cursor: pointer;
    transition:
      color 150ms ease,
      text-decoration-color 150ms ease;
  }

  .chat-presence-continue:hover {
    color: rgb(var(--color-surface-200));
    text-decoration-color: rgb(var(--color-surface-400) / 0.7);
  }

  .chat-presence-empty--exiting {
    opacity: 0;
    transition:
      opacity 420ms cubic-bezier(0.22, 1, 0.36, 1);
    pointer-events: none;
  }
</style>
