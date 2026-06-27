<script lang="ts">
  import { ArrowDown, ExternalLink, LoaderCircle, PanelLeft, Users } from "@lucide/svelte";
  import ChatMessageList from "$lib/components/chat/ChatMessageList.svelte";
  import ChatComposerBar from "$lib/components/chat/ChatComposerBar.svelte";
  import BudgetApprovalBar from "$lib/components/chat/BudgetApprovalBar.svelte";
  import AgentBrowserPanel from "$lib/components/chat/AgentBrowserPanel.svelte";
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
    createTurnTicket,
  } from "$lib/daemon";

  import { formatSessionLabel } from "$lib/utils/formatSession";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import { STARTER_PROMPTS } from "$lib/utils/starterPrompts";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { groupAskThreads, isChatLaneMessage } from "$lib/utils/askThreads";
  import { groupWorkerThreads } from "$lib/utils/workerThreads";
  import {
    parseChatSlashInput,
    runSlashCommand,
  } from "$lib/utils/runSlashCommand";
  import { SLASH_COMMAND_HINTS } from "$lib/utils/slashCommands";
  import { isTauri, showChatPopout } from "$lib/window";
  import OfflineChatGate from "$lib/components/chat/OfflineChatGate.svelte";
  import { pendingMediaLabels } from "$lib/utils/chatMediaUpload";
  import { hasVisionMediaRefs } from "$lib/types/media";
  import { visionProfileReady } from "$lib/types/inferenceProfiles";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { flowDraft } from "$lib/stores/flowDraft.svelte";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";

  interface Props {
    visible: boolean;
    showPopout?: boolean;
    mobile?: boolean;
    embedded?: boolean;
    workshop?: boolean;
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
    scriptWorkbench = false,
    onOpenContext,
    onOpenConnection,
  }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();
  let atBottom = $state(true);

  const scrollPinThresholdPx = $derived(mobile ? 24 : 96);

  const chatMessages = $derived(chat.messages.filter((message) => isChatLaneMessage(message)));
  const askThreads = $derived(groupAskThreads(chat.messages));
  const workerThreads = $derived(groupWorkerThreads(chat.messages));
  const showInlineComposer = $derived(!mobile || (embedded && scriptWorkbench));
  const useMobileChatLayout = $derived(mobile);
  const showChatEmptyState = $derived(
    chatMessages.length === 0 && askThreads.length === 0 && workerThreads.length === 0,
  );
  function handlePromoteToFlow(ref: ToolHistorySliceRef) {
    flowDraft.queuePromotion([ref]);
    automationsNav.openSection("flows");
    layout.navigateDesktop("automations", { bump: true });
    if (mobile) layout.openMore("automations");
  }
  const sessionLabel = $derived.by(() => {
    const session = chat.sessions.find((entry) => entry.session_id === chat.sessionId);
    if (session) return formatSessionLabel(session);
    return formatSessionLabel({
      session_id: chat.sessionId,
      preview: "",
      turns: 0,
      verification_runs: 0,
    });
  });
  const recentSessions = $derived(
    chat.sessions.filter((session) => session.session_id !== chat.sessionId).slice(0, 4),
  );
  const streamingMessage = $derived(
    chat.messages.find((message) => message.streaming && message.role === "assistant"),
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
    if (showChatEmptyState) return "What are you working on?";
    if (chat.historyLoading && chat.messages.length === 0) return "Opening thread…";
    const last = [...chat.messages].reverse().find((message) => message.content.trim());
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
    void chat.sessionId;
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

  async function submit(event: Event) {
    event.preventDefault();
    if (connection.offline) return;
    const prompt = chat.draft.trim();
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

  async function sendStarterPrompt(prompt: string) {
    if (connection.offline || chat.composerBlocked) return;
    if (mobile) haptic("light");
    try {
      const mode = chat.hasLiveInteractiveTurn() ? "background" : "interactive";
      await submitTurn(prompt, prompt, mode);
    } catch (err) {
      chat.setError(err instanceof Error ? err.message : String(err));
    }
  }
</script>

<section
  class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible
    ? ''
    : 'hidden'} {embedded && useMobileChatLayout
    ? 'script-workbench-chat-mobile-root'
    : embedded
      ? 'vault-workshop-chat-panel'
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
          <button
            type="button"
            class="workshop-rail-btn shrink-0"
            aria-label="Open sessions"
            title="Sessions"
            onclick={() => layout.toggleSessionDrawer()}
          >
            <PanelLeft size={16} strokeWidth={1.75} />
          </button>
        {/if}
        <button
          type="button"
          class="min-w-0 text-left {mobile ? 'py-1' : ''}"
          onclick={() => layout.toggleSessionDrawer()}
        >
          {#if mobile}
            <h1 class="truncate text-sm font-semibold text-surface-50">
              {mobileChatTitle}
            </h1>
            <p class="truncate text-[11px] text-surface-400">{mobileChatSubtitle}</p>
          {:else}
            <h1 class="truncate text-sm font-semibold text-surface-50">{sessionLabel}</h1>
            <p class="workshop-header-line truncate">Talk with her in this session</p>
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
    </div>
    {#if chat.streamError}
      <p class="mt-1 text-[11px] text-error-400" role="alert">{chat.streamError}</p>
    {:else if !mobile && chat.historyLoading && chat.messages.length === 0}
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
          : 'chat-scroll space-y-4'}"
    >
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
                  sessionId={chat.sessionId}
                  {mobile}
                  compact={true}
                  onPromoteToFlow={handlePromoteToFlow}
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
                  sessionId={chat.sessionId}
                  {mobile}
                  compact={true}
                  onPromoteToFlow={handlePromoteToFlow}
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
          sessionId={chat.sessionId}
          {mobile}
          onPromoteToFlow={handlePromoteToFlow}
        />
      {:else if showChatEmptyState}
      <div
        class="flex min-h-[120px] flex-col justify-center {embedded ? 'px-3 py-2' : mobile ? 'px-1 pb-4' : 'px-2'}"
      >
        {#if scriptWorkbench && chat.scriptWorkbenchContext}
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
        {:else if workshop && chat.vaultNoteContext}
          <p class="text-sm text-surface-400">Ask about this note — links, edits, or next steps.</p>
          <div class="mt-3 flex flex-wrap gap-2">
            {#each ["What links here?", "Summarize this note", "Suggest edits"] as prompt (prompt)}
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
        {:else}
        <p class="text-sm text-surface-400 {mobile ? '' : 'mt-8'}">What are you working on?</p>
        <div class="mt-4 flex flex-wrap gap-2">
          {#each STARTER_PROMPTS as prompt (prompt)}
            <button
              type="button"
              class="rounded-full border border-surface-500/40 bg-surface-950/50 px-3 py-1.5 text-sm text-surface-200 transition hover:border-primary-400/50 hover:text-surface-50"
              disabled={connection.offline || chat.composerBlocked}
              onclick={() => void sendStarterPrompt(prompt)}
            >
              {prompt}
            </button>
          {/each}
        </div>
        {/if}
        {#if recentSessions.length > 0 && !embedded}
          <ul class="mt-5 space-y-1.5">
            {#each recentSessions as session (session.session_id)}
              <li>
                <button
                  type="button"
                  class="workshop-text-action block max-w-md truncate text-left"
                  onclick={() => resumeSession(session.session_id)}
                >
                  {formatSessionLabel(session)}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="mt-3 text-sm text-surface-400">No prior sessions</p>
        {/if}
      </div>
      {:else if chat.historyLoading && chat.messages.length === 0 && !mobile}
      <div class="flex min-h-[200px] items-center justify-center">
        <LoaderCircle size={22} class="animate-spin text-surface-500/80" aria-label="Loading" />
      </div>
      {/if}
    </div>
    {#if !useMobileChatLayout}
      <div class="chat-scroll-fade" aria-hidden="true"></div>
    {/if}
  </div>

  {#if showInlineComposer}
    {#if !embedded}
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
          : 'vault-workshop-chat-composer'
        : 'chat-composer'}"
      onsubmit={submit}
    >
      {#if chat.scriptWorkbenchContext}
        <ScriptChatContextChip compact={workshop || scriptWorkbench} class={embedded ? "mb-2" : "mx-4 mb-2"} />
      {:else if chat.vaultNoteContext}
        <VaultChatContextChip compact={workshop} class={workshop ? "mb-2" : "mx-4 mb-2"} />
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
      <ChatComposerBar
        mobile={workshop || useMobileChatLayout}
        disabled={connection.offline}
        composerBlocked={chat.composerBlocked}
        onkeydown={handleKeydown}
      />
    </form>
  {/if}

  {#if visible && connection.offline}
    <OfflineChatGate {mobile} {onOpenConnection} />
  {/if}

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
