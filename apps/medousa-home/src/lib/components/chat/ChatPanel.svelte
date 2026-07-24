<script lang="ts">
  import { tick } from "svelte";
  import { ArrowDown, ExternalLink, LoaderCircle, PanelLeft, Users } from "@lucide/svelte";
  import ChatAsyncToolsHint from "$lib/components/chat/ChatAsyncToolsHint.svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import VaultChatContextChip from "$lib/components/vault/VaultChatContextChip.svelte";
  import ScriptChatContextChip from "$lib/components/grapheme/ScriptChatContextChip.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { voicePresets } from "$lib/stores/voicePresets.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import {
    createAgentSession,
    createTurnTicket,
    steerBoundWorkshop,
  } from "$lib/daemon";
  import {
    agentRuntimeLabel,
    getSessionAgentRuntime,
    setSessionAgentRuntime,
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
  import { groupWorkerThreads } from "$lib/utils/workerThreads";
  import {
    saveChatTurnToVault,
    showChatTurnSaveFeedback,
  } from "$lib/utils/saveChatTurnToVault";
  import type { ChatMessage } from "$lib/types/chat";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import { SLASH_COMMAND_HINTS } from "$lib/utils/slashCommands";
  import { isTauri, showChatPopout } from "$lib/window";
  import OfflineChatGate from "$lib/components/chat/OfflineChatGate.svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import { hasVisionMediaRefs } from "$lib/types/media";
  import { visionProfileReady } from "$lib/types/inferenceProfiles";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

  interface Props {
    visible: boolean;
    showPopout?: boolean;
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
    showPopout = true,
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
  let cardDetailOpen = $state(false);
  let cardDetail = $state<CardDetailPayload | null>(null);

  function openCardDetail(detail: CardDetailPayload) {
    cardDetail = detail;
    cardDetailOpen = true;
  }

  function closeCardDetail() {
    cardDetailOpen = false;
    cardDetail = null;
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
  const panelMessages = $derived(chat.messagesFor(panelSessionId));
  const chatMessages = $derived(panelMessages.filter((message) => isChatLaneMessage(message)));
  const askThreads = $derived(groupAskThreads(panelMessages));
  const workerThreads = $derived(groupWorkerThreads(panelMessages));
  const showInlineComposer = $derived(!mobile || (embedded && scriptWorkbench));
  const useMobileChatLayout = $derived(mobile);
  const showChatEmptyState = $derived(
    chatMessages.length === 0 && askThreads.length === 0 && workerThreads.length === 0,
  );

  /** Presence — the quiet, centered landing for a genuinely empty main chat. */
  const showPresenceEmpty = $derived(
    showChatEmptyState && !workshop && !scriptWorkbench && !embedded,
  );
  const presenceAsk = $derived(presenceSubline());

  let presenceDockMode = $state<"center" | "docking" | "docked">("docked");
  let presenceDockEl = $state<HTMLDivElement | undefined>(undefined);
  let presenceBlurpToken = 0;
  let presenceDockLocked = $state(false);
  /** translateY offset that parks the bottom-anchored dock in visual center. */
  let presenceCenterOffset = $state(0);
  let presenceCenterPlaced = $state(false);

  const presenceComposerCentered = $derived(
    showPresenceEmpty &&
      showInlineComposer &&
      (presenceDockMode === "center" || presenceDockMode === "docking"),
  );

  function handlePromoteToFlow(ref: ToolHistorySliceRef) {
    flowDraft.queuePromotion([ref]);
    automationsNav.openSection("flows");
    layout.navigateDesktop("automations", { bump: true });
    if (mobile) layout.openMore("automations");
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
    const session = chat.sessions.find((entry) => entry.session_id === panelSessionId);
    const label = session
      ? formatSessionLabel(session)
      : formatSessionLabel({
          session_id: panelSessionId,
          preview: "",
          turns: 0,
          verification_runs: 0,
        });
    if (showChatEmptyState && label === "New conversation") return presenceRoomTitle();
    return label;
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
    mobile &&
      !atBottom &&
      (chatMessages.length > 0 || askThreads.length > 0 || workerThreads.length > 0),
  );

  $effect(() => {
    void panelSessionId;
    atBottom = true;
  });

  $effect(() => {
    if (!scrollEl) return;
    void chatMessages.map((message) => message.content).join("\0");
    void askThreads
      .flatMap((thread) => thread.messages.map((message) => message.content))
      .join("\0");
    void workerThreads
      .flatMap((thread) => thread.messages.map((message) => message.content))
      .join("\0");
    void chat.hasTurnActivity;
    scrollToLatest(false);
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
    presenceDockLocked = false;
    presenceCenterOffset = 0;
    presenceCenterPlaced = false;
    if (presenceDockMode !== "docking") {
      presenceDockMode = "docked";
    }
  });

  /** Bottom-anchored dock + translateY to visual center (no layout FLIP). */
  async function placePresenceCentered() {
    presenceCenterPlaced = false;
    await tick();
    const el = presenceDockEl;
    const parent = el?.parentElement;
    if (!el || !parent || presenceDockMode !== "center") return;

    el.style.transition = "none";
    el.style.transform = "translate3d(0, 0, 0)";
    void el.offsetHeight;

    const parentRect = parent.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const centeredTop = parentRect.top + (parentRect.height - elRect.height) / 2;
    const offset = centeredTop - elRect.top;
    presenceCenterOffset = offset;
    el.style.transform = `translate3d(0, ${offset}px, 0)`;
    presenceCenterPlaced = true;
  }

  $effect(() => {
    if (presenceDockMode !== "center" || presenceDockLocked) return;
    void placePresenceCentered();
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
      if (el) {
        el.getAnimations().forEach((animation) => animation.cancel());
        el.style.transition = "";
        el.style.transform = "";
        el.style.transformOrigin = "";
      }
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
    presenceCenterOffset = 0;
    presenceDockMode = "docked";
  }

  function parseDaemonAskPrompt(value: string): string | null {
    const slash = parseChatSlashInput(value);
    if (slash?.kind === "ask") return slash.prompt;
    return null;
  }

  const slashHint = $derived.by(() => {
    const draft = chat.draft.trim();
    if (!draft.startsWith("/")) return null;
    return SLASH_COMMAND_HINTS.filter((hint) =>
      hint.toLowerCase().startsWith(draft.toLowerCase()),
    ).slice(0, 4);
  });

  async function submitTurn(userContent: string, prompt: string, mode: "interactive" | "background") {
    const runtime = getSessionAgentRuntime(chat.sessionId);
    if (runtime !== "medousa" && mode === "interactive") {
      const acceptedAgent = await createAgentSession({
        session_id: chat.sessionId,
        runtime,
        prompt,
      });
      const ticket: TurnTicketResponse = {
        turn_id: acceptedAgent.agent_session_id,
        session_id: acceptedAgent.session_id,
        mode: "interactive",
        phase: "accepted" as TurnTicketResponse["phase"],
        accepted_at_utc: acceptedAgent.accepted_at_utc ?? new Date().toISOString(),
        stream_url: acceptedAgent.stream_url,
        stream_ready: acceptedAgent.stream_ready,
      };
      chat.beginTurn(userContent, ticket, []);
      chat.clearPendingMedia();
      scrollToLatest(true);
      await chat.startTurnStream(
        ticket.turn_id,
        ticket.session_id,
        ticket.stream_url,
      );
      return;
    }

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

  $effect(() => {
    sessionRuntime = getSessionAgentRuntime(chat.sessionId);
  });

  function onRuntimeChange(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value as ChatAgentRuntime;
    sessionRuntime = value;
    setSessionAgentRuntime(chat.sessionId, value);
  }

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline) return;
    const scopeForSend = chat.vaultNoteContext;
    const prompt = applyActiveAgentPrompt(
      ensureVaultSelectionInPrompt(chat.draft.trim(), scopeForSend),
    );
    const hasAttachments = chat.pendingMediaRefs.length > 0;
    if (!prompt && !hasAttachments) return;
    if (hasVisionMediaRefs(chat.pendingMediaRefs)) {
      if (!workshopDefaults.loaded) {
        await workshopDefaults.load();
      }
      if (!visionProfileReady(workshopDefaults.draft.inferenceProfiles)) {
        chat.setError("Configure a vision model in Settings → Models before sending images.");
        return;
      }
    }
    if (mobile) haptic("medium");

    const askPrompt = parseDaemonAskPrompt(prompt);
    const slash = parseChatSlashInput(prompt);
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
        await submitTurn(prompt || pendingMediaLabels(chat.pendingMediaRefs), askPrompt, "background");
        return;
      }

      if (chat.hasWorkshopHandoff()) {
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

  function onScroll() {
    if (!scrollEl) return;
    const distanceFromBottom =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    atBottom = distanceFromBottom <= scrollPinThresholdPx;
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
    try {
      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      const fullPrompt = ensureVaultSelectionInPrompt(prompt, chat.vaultNoteContext);
      await submitTurn(fullPrompt, fullPrompt, mode);
    } catch (err) {
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
    <div class="flex items-center justify-between gap-3">
      <div class="flex min-w-0 items-center gap-2">
        {#if mobile}
          <button
            type="button"
            class="mobile-icon-btn shrink-0"
            aria-label="Open sessions"
            onclick={() => layout.toggleSessionDrawer()}
          >
            <PanelLeft size={20} strokeWidth={1.75} />
          </button>
        {:else}
          <ShellSidebarExpandButton label="Show sessions" />
        {/if}
        <button
          type="button"
          class="min-w-0 text-left {mobile ? 'py-1' : ''}"
          onclick={() => {
            if (mobile) {
              layout.toggleSessionDrawer();
              return;
            }
            if (!layout.shellSidebarExpanded) {
              layout.openShellSidebarView("chat");
            }
          }}
        >
          {#if mobile}
            <h1 class="truncate text-sm font-semibold text-surface-50">
              {mobileChatTitle}
            </h1>
            <p class="truncate text-[11px] text-surface-400">{mobileChatSubtitle}</p>
          {:else}
            <h1 class="truncate text-sm font-semibold text-surface-50">{sessionLabel}</h1>
          {/if}
        </button>
        {#if chat.hasTurnActivity && mobile}
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
      </div>
      {#if mobile || !showChatEmptyState}
        <div class="flex shrink-0 items-center gap-0.5">
          <button
            type="button"
            class="{mobile ? 'mobile-icon-btn' : 'workshop-rail-btn'}"
            aria-label="Identity recall"
            title="Identity recall"
            onclick={() => layout.toggleIdentityDrawer()}
          >
            <Users size={mobile ? 20 : 16} strokeWidth={1.75} />
          </button>
          {#if showPopout && isTauri()}
            <button
              type="button"
              class="workshop-rail-btn"
              aria-label="Pop out chat"
              title="Pop out"
              onclick={() => showChatPopout()}
            >
              <ExternalLink size={16} strokeWidth={1.75} />
            </button>
          {/if}
        </div>
      {/if}
    </div>
    {#if chat.streamErrorFor(panelSessionId)}
      <p class="mt-1 text-[11px] text-error-400" role="alert">{chat.streamErrorFor(panelSessionId)}</p>
    {:else if !mobile && chat.historyLoadingFor(panelSessionId) && panelMessages.length === 0}
      <p class="mt-1 text-[11px] text-surface-400">Loading conversation…</p>
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
          class="chat-restore-toast-dismiss text-primary-300"
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

  <div class="chat-panel-main">
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
      {#if askThreads.length > 0 && !embedded}
        {#if mobile}
          <button
            type="button"
            class="mobile-chat-rail-chip"
            onclick={() => switchMobileTab("home")}
          >
            <span>
              {askThreads.length} background ask{askThreads.length === 1 ? "" : "s"} in Work
            </span>
            <span class="text-surface-500">→</span>
          </button>
        {:else}
        <section class="chat-ask-rail space-y-3">
          <div class="chat-ask-rail-header">
            <p class="text-[11px] font-medium uppercase tracking-[0.14em] text-surface-500">
              Asks
            </p>
            <p class="mt-0.5 text-[11px] text-surface-500">
              Scoped work — separate from this conversation
            </p>
          </div>
          {#each askThreads as thread (thread.jobId)}
            <article class="chat-ask-thread">
              <header class="chat-ask-thread-header">
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium text-surface-100">
                    {thread.promptPreview}
                  </p>
                  <p class="mt-0.5 text-[10px] text-surface-500">
                    {#if thread.active}
                      In progress
                    {:else}
                      Settled
                    {/if}
                  </p>
                </div>
                {#if !thread.active}
                  <button
                    type="button"
                    class="workshop-text-action shrink-0 text-[11px]"
                    onclick={() => chat.promoteAskToChat(thread.jobId)}
                  >
                    Move to chat
                  </button>
                {/if}
              </header>
              <div class="chat-ask-thread-body space-y-3">
                <ChatMessageList
                  messages={thread.messages}
                  sessionId={panelSessionId}
                  {mobile}
                  compact={true}
                  scrollRoot={scrollEl}
                  onPromoteToFlow={handlePromoteToFlow}
                  onSubmitIntent={submitChatIntent}
                  onSaveToVault={handleSaveToVault}
                  onOpenCardDetail={openCardDetail}
                />
              </div>
            </article>
          {/each}
        </section>
        {/if}
      {/if}

      {#if workerThreads.length > 0 && !embedded}
        {#if mobile}
          <button
            type="button"
            class="mobile-chat-rail-chip"
            onclick={() => switchMobileTab("home")}
          >
            <span>
              {workerThreads.length} worker{workerThreads.length === 1 ? "" : "s"} in Work
            </span>
            <span class="text-surface-500">→</span>
          </button>
        {:else}
        <section class="chat-ask-rail space-y-3">
          <div class="chat-ask-rail-header">
            <p class="text-[11px] font-medium uppercase tracking-[0.14em] text-surface-500">
              Workers
            </p>
            <p class="mt-0.5 text-[11px] text-surface-500">
              Workshop lane — progress stays off the main thread
            </p>
          </div>
          {#each workerThreads as thread (thread.workId)}
            <article class="chat-ask-thread">
              <header class="chat-ask-thread-header">
                <div class="min-w-0">
                  <p class="truncate text-sm font-medium text-surface-100">
                    {thread.workId}
                  </p>
                  <p class="mt-0.5 text-[10px] text-surface-500">
                    {#if thread.active}
                      {visibleChatStatusLine(thread.statusLine, settings.showEngineDetailsInChat) ??
                        "Working in background…"}
                    {:else}
                      Settled
                    {/if}
                  </p>
                </div>
              </header>
              <div class="chat-ask-thread-body space-y-3">
                <ChatMessageList
                  messages={thread.messages}
                  sessionId={panelSessionId}
                  {mobile}
                  compact={true}
                  workerThread={true}
                  scrollRoot={scrollEl}
                  onPromoteToFlow={handlePromoteToFlow}
                  onSubmitIntent={submitChatIntent}
                  onSaveToVault={handleSaveToVault}
                  onOpenCardDetail={openCardDetail}
                />
              </div>
            </article>
          {/each}
        </section>
        {/if}
      {/if}

      {#if chatMessages.length > 0}
        <ChatMessageList
          messages={chatMessages}
          sessionId={panelSessionId}
          {mobile}
          scrollRoot={scrollEl}
          onPromoteToFlow={handlePromoteToFlow}
          onSubmitIntent={submitChatIntent}
          onSaveToVault={handleSaveToVault}
          onOpenCardDetail={openCardDetail}
        />
      {:else if showChatEmptyState}
        {#if scriptWorkbench && chat.scriptWorkbenchContext}
        <div
          class="flex min-h-[120px] flex-col justify-center {embedded ? 'px-3 py-2' : mobile ? 'px-1 pb-4' : 'px-2'}"
        >
          <p class="text-sm text-surface-400">Ask about this script — fixes, modules, or next steps.</p>
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
            <p class="px-1 text-[12px] leading-relaxed text-surface-500">
              {vaultContextHasSelection(chat.vaultNoteContext)
                ? "Ask about this passage…"
                : "Ask about this note…"}
            </p>
          {:else}
            <p class="text-sm text-surface-400">
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
        <LoaderCircle size={22} class="animate-spin text-surface-500/80" aria-label="Loading" />
      </div>
      {/if}
    </div>
    {#if !useMobileChatLayout && !presenceComposerCentered}
      <div class="chat-scroll-fade" aria-hidden="true"></div>
    {/if}
  </div>

  {#if showInlineComposer}
  <div
    bind:this={presenceDockEl}
    class="chat-presence-dock chat-presence-dock--{presenceDockMode}"
    class:chat-presence-dock--placed={presenceCenterPlaced ||
      presenceDockMode === "docking" ||
      presenceDockMode === "docked"}
  >
    {#if showPresenceEmpty && (presenceDockMode === "center" || presenceDockMode === "docking")}
      <div class="chat-presence-empty {presenceDockMode === 'docking' ? 'chat-presence-empty--exiting' : ''}">
        <p class="chat-presence-ask">{presenceAsk}</p>
        {#if continueSession}
          <button
            type="button"
            class="chat-presence-continue"
            onclick={() => void continueWhereLeftOff()}
          >
            Continue where we left off
          </button>
        {/if}
      </div>
    {/if}
    {#if !embedded && presenceDockMode === "docked"}
    <BudgetApprovalBar
      onOpenWork={() => {
        workspace.workView = "kanban";
        const pending = chat.budgetAlert ?? chat.pendingBudgetApprovals[0];
        if (pending) void workspace.selectCard(pending.workCardId);
      }}
    />
    <AgentBrowserPanel />
    {/if}
    <form
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
      {#if slashHint?.length}
        <ul
          class="{workshop ? 'mb-1' : 'mx-4 mb-1'} space-y-0.5 text-[11px] text-surface-500"
        >
          {#each slashHint as hint (hint)}
            <li>{hint}</li>
          {/each}
        </ul>
      {/if}
      {#if chat.hasWorkshopHandoff()}
        <p
          class="{workshop ? 'mb-1.5' : 'mx-4 mb-1.5'} text-[11px] font-medium text-primary-300/90"
        >
          Steering handoff — your next message continues the worker
        </p>
      {/if}
      {#if !workshop && !embedded && !showChatEmptyState}
        <div class="mx-4 mb-1.5 flex items-center gap-2 text-[11px] text-surface-400">
          <label for="chat-agent-runtime" class="sr-only">Model</label>
          <select
            id="chat-agent-runtime"
            class="rounded-md border border-surface-500/30 bg-surface-900/40 px-2 py-0.5 text-surface-200"
            value={sessionRuntime}
            onchange={onRuntimeChange}
            disabled={connection.offline || chat.composerBlocked}
          >
            <option value="medousa">{agentRuntimeLabel("medousa")}</option>
            <option value="cursor">{agentRuntimeLabel("cursor")}</option>
            <option value="codex">{agentRuntimeLabel("codex")}</option>
          </select>
        </div>
      {/if}
      <ChatComposerBar
        mobile={workshop || useMobileChatLayout}
        disabled={connection.offline}
        composerBlocked={chat.composerBlocked}
        quietChrome={showPresenceEmpty || presenceDockMode === "docking"}
        onkeydown={handleKeydown}
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

  {#if showScrollFab && visible}
    <button
      type="button"
      class="mobile-chat-scroll-fab"
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

  .chat-presence-empty {
    position: absolute;
    bottom: calc(100% + 0.75rem);
    left: 50%;
    display: flex;
    width: max-content;
    max-width: min(24rem, 100%);
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
    text-align: center;
    transform: translateX(-50%);
  }

  .chat-presence-ask {
    margin: 0;
    max-width: 24rem;
    font-size: clamp(1.15rem, 2vw, 1.4rem);
    font-weight: 500;
    line-height: 1.35;
    letter-spacing: -0.015em;
    color: rgb(var(--color-surface-100));
  }

  .chat-presence-continue {
    border: 0;
    background: transparent;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-400));
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
    transform: translateX(-50%) translateY(-0.5rem);
    transition:
      opacity 420ms cubic-bezier(0.22, 1, 0.36, 1),
      transform 420ms cubic-bezier(0.22, 1, 0.36, 1);
    pointer-events: none;
  }
</style>
