<script lang="ts">
  import { ExternalLink, PanelLeft, Users } from "@lucide/svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import ToolRunChips from "$lib/components/chat/ToolRunChips.svelte";
  import AssistantThinking from "$lib/components/chat/AssistantThinking.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    createTurnTicket,
    startInteractiveStream,
  } from "$lib/daemon";

  import {
    answerStateTextClass,
    formatAnswerState,
  } from "$lib/utils/formatAnswer";
  import { formatSessionLabel } from "$lib/utils/formatSession";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { isTauri, showChatPopout } from "$lib/window";

  interface Props {
    visible: boolean;
    showPopout?: boolean;
    mobile?: boolean;
  }

  let { visible, showPopout = true, mobile = false }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();

  const sessionLabel = $derived(chat.currentSessionLabel());
  const recentSessions = $derived(
    chat.sessions.filter((session) => session.session_id !== chat.sessionId).slice(0, 4),
  );
  const streamingMessage = $derived(
    chat.messages.find((message) => message.streaming && message.role === "assistant"),
  );
  const phaseLine = $derived.by(() => {
    if (!streamingMessage) return null;
    if (streamingMessage.statusLine?.trim()) return streamingMessage.statusLine.trim();
    if (streamingMessage.phase) return formatTurnPhase(streamingMessage.phase);
    if (streamingMessage.tools?.length) {
      return streamingMessage.tools.map((tool) => formatToolName(tool)).join(" · ");
    }
    return "Working…";
  });

  const mobileChatTitle = $derived.by(() => {
    if (!mobile) return "Medousa";
    if (chat.liveStreamActive && phaseLine) return phaseLine;
    if (chat.backgroundActivity > 0) {
      return chat.backgroundActivity === 1
        ? "Working in background"
        : `${chat.backgroundActivity} turns active`;
    }
    return "Medousa";
  });

  const mobileChatSubtitle = $derived.by(() => {
    if (!mobile) return sessionLabel;
    if (chat.liveStreamActive) return "Thinking…";
    if (chat.backgroundActivity > 0) return "Composer open · check Work";
    const last = [...chat.messages].reverse().find((message) => message.content.trim());
    if (last?.content) {
      const line = last.content.trim().split("\n")[0];
      return line.length > 56 ? `${line.slice(0, 55)}…` : line;
    }
    return "Say one thing";
  });

  $effect(() => {
    if (scrollEl && (chat.messages.length || chat.hasTurnActivity)) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  $effect(() => {
    if (!mobile || !visible) return;
    const onComposerFocus = () => scrollToLatest();
    window.addEventListener("medousa-chat-composer-focus", onComposerFocus);
    return () => window.removeEventListener("medousa-chat-composer-focus", onComposerFocus);
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
    const trimmed = value.trim();
    if (trimmed.startsWith("/ask ")) return trimmed.slice(5).trim();
    if (trimmed.startsWith("/daemon ask ")) return trimmed.slice(12).trim();
    return null;
  }

  async function submitTurn(userContent: string, prompt: string, mode: "interactive" | "background") {
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
    await startInteractiveStream(accepted.stream_url);
  }

  async function submit(event: Event) {
    event.preventDefault();
    const prompt = chat.draft.trim();
    if (!prompt) return;
    if (mobile) haptic("medium");

    const askPrompt = parseDaemonAskPrompt(prompt);
    chat.draft = "";

    try {
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

  function scrollToLatest() {
    if (!scrollEl) return;
    requestAnimationFrame(() => {
      scrollEl!.scrollTop = scrollEl!.scrollHeight;
    });
  }

  async function resumeSession(sessionId: string) {
    await chat.switchSession(sessionId);
  }
</script>

<section
  class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col {visible
    ? ''
    : 'hidden'} {mobile ? 'mobile-chat-panel' : ''}"
>
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
            <h1 class="text-sm font-semibold text-surface-50">Chat</h1>
            <p class="truncate text-[11px] text-surface-400" title={chat.sessionId}>
              {sessionLabel}
            </p>
          {/if}
        </button>
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
      <p class="mt-1 text-[11px] text-error-400">{chat.streamError}</p>
    {:else if chat.historyLoading}
      <p class="mt-1 text-[11px] text-surface-400">Loading conversation from Mac…</p>
    {/if}
  </header>

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
    bind:this={scrollEl}
    class="{mobile
      ? 'mobile-chat-scroll space-y-3'
      : 'flex-1 space-y-3 overflow-y-auto px-4 py-3'}"
  >
    {#each chat.messages as message (message.id)}
      <article
        class="{mobile && message.role === 'user'
          ? 'mobile-chat-bubble-user'
          : mobile && message.role === 'assistant'
            ? 'mobile-chat-bubble-assistant'
            : ''} max-w-2xl {message.role === 'user'
          ? mobile
            ? ''
            : 'user-turn ml-auto max-w-[min(100%,42rem)] rounded-lg border border-primary-500/15 bg-gradient-to-br from-primary-500/10 via-primary-500/[0.06] to-surface-900/40 px-3.5 py-2.5 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]'
          : message.role === 'system'
            ? 'workshop-faint px-1'
            : mobile
              ? ''
              : 'assistant-turn border-l-2 border-primary-500/25 pl-3'}"
      >
        {#if message.role === "assistant"}
          {#if message.reasoning?.trim()}
            <AssistantThinking
              reasoning={message.reasoning}
              streaming={Boolean(message.streaming)}
              compact={mobile}
            />
          {:else if message.streaming && !message.content?.trim()}
            <div
              class="mb-2 flex items-center gap-2 rounded-lg border border-primary-500/15 bg-gradient-to-r from-primary-500/[0.05] to-surface-900/20 px-2.5 py-1.5"
            >
              <span
                class="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-primary-400"
                aria-hidden="true"
              ></span>
              <span class="text-[11px] text-primary-200/90">Thinking…</span>
            </div>
          {/if}

          {#if message.statusLine && (message.streaming || message.phase === "worker_ack" || message.phase === "awaiting_operator")}
            <p
              class="mb-2 flex items-center gap-1.5 text-[11px] {message.phase === 'worker_ack' ||
              message.phase === 'awaiting_operator'
                ? 'text-warning-300/90'
                : 'text-primary-200/80'}"
            >
              {#if message.streaming}
                <span
                  class="inline-block h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-primary-400"
                  aria-hidden="true"
                ></span>
              {/if}
              {message.statusLine}
            </p>
          {/if}

          {@const answer = formatAnswerState(message.answerState)}
          <div class="text-surface-50">
            <MarkdownContent content={message.content || "…"} />
          </div>

          {#if answer && !message.streaming}
            <p class="mt-1.5 text-[10px] {answerStateTextClass(answer.tone)}">
              {answer.label}
            </p>
          {/if}

          {#if message.toolRuns && message.toolRuns.length > 0}
            <div class="mt-3">
              <ToolRunChips
                runs={message.toolRuns}
                compact={mobile}
                inspectorCollapsed={!message.streaming}
              />
            </div>
          {:else if message.tools && message.tools.length > 0}
            <p class="mt-3 font-mono text-[10px] text-surface-500">
              {message.tools.map((tool) => formatToolName(tool)).join(" · ")}
            </p>
          {/if}
        {:else if message.role === "user"}
          <p class="whitespace-pre-wrap text-sm leading-relaxed text-surface-100">
            {message.content}
          </p>
        {:else}
          <p class="whitespace-pre-wrap text-sm leading-relaxed text-surface-300">
            {message.content}
          </p>
        {/if}
      </article>
    {:else}
      <div class="flex h-full min-h-[200px] flex-col justify-center px-2">
        {#if mobile}
          <p class="text-sm text-surface-300">Say one thing.</p>
          <p class="workshop-faint mt-2 text-xs">Medousa remembers this conversation.</p>
          {#if recentSessions.length > 0}
            <ul class="mt-6 space-y-2">
              {#each recentSessions as session (session.session_id)}
                <li>
                  <button
                    type="button"
                    class="workshop-text-action block max-w-md truncate text-left text-sm"
                    onclick={() => resumeSession(session.session_id)}
                  >
                    {formatSessionLabel(session)}
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          <p class="workshop-faint font-mono text-[11px]">
            {runtime.modelLabel()} · depth {runtime.depthMode}
          </p>
          {#if recentSessions.length > 0}
            <ul class="mt-4 space-y-1">
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
        {/if}
      </div>
    {/each}
  </div>

  {#if !mobile}
    <form class="workshop-composer" onsubmit={submit}>
      <div class="composer-bar">
        <GrowingTextarea
          bind:value={chat.draft}
          placeholder="Message · /ask for background jobs"
          disabled={chat.composerBlocked}
          maxHeight={128}
          minHeight={36}
          onkeydown={handleKeydown}
          aria-label="Message"
        />
        <button
          type="submit"
          class="composer-bar-send"
          disabled={chat.composerBlocked || !chat.draft.trim()}
          aria-label="Send message"
        >
          {chat.composerBlocked ? "…" : "↑"}
        </button>
      </div>
    </form>
  {/if}
</section>
