<script lang="ts">
  import { ExternalLink, PanelLeft, Users } from "@lucide/svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import {
    enqueueDaemonAsk,
    sendInteractiveTurn,
    startInteractiveStream,
    stopInteractiveStream,
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
  }

  let { visible, showPopout = true }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();

  const sessionLabel = $derived(chat.currentSessionLabel());
  const recentSessions = $derived(
    chat.sessions.filter((session) => session.session_id !== chat.sessionId).slice(0, 4),
  );

  $effect(() => {
    if (scrollEl && (chat.messages.length || chat.isStreaming)) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  function parseDaemonAskPrompt(value: string): string | null {
    const trimmed = value.trim();
    if (trimmed.startsWith("/ask ")) return trimmed.slice(5).trim();
    if (trimmed.startsWith("/daemon ask ")) return trimmed.slice(12).trim();
    return null;
  }

  async function submit(event: Event) {
    event.preventDefault();
    const prompt = chat.draft.trim();
    if (!prompt || chat.isStreaming) return;

    const askPrompt = parseDaemonAskPrompt(prompt);
    chat.draft = "";

    if (askPrompt) {
      chat.messages = [
        ...chat.messages,
        { id: crypto.randomUUID(), role: "user", content: prompt },
      ];
      try {
        const accepted = await enqueueDaemonAsk(askPrompt, runtime.model);
        chat.messages = [
          ...chat.messages,
          {
            id: crypto.randomUUID(),
            role: "system",
            content: `Queued background ask · job ${accepted.job_id} on ${accepted.queue}. Watch Work for the card.`,
          },
        ];
      } catch (err) {
        chat.setError(err instanceof Error ? err.message : String(err));
      }
      return;
    }

    chat.beginUserMessage(prompt);

    try {
      await stopInteractiveStream();
      const accepted = await sendInteractiveTurn(chat.sessionId, prompt, {
        provider: runtime.provider,
        model: runtime.model,
        responseDepthMode: runtime.depthMode,
        stageRouting: runtime.stageRouting,
      });
      await startInteractiveStream(accepted.stream_url);
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

  async function resumeSession(sessionId: string) {
    await chat.switchSession(sessionId);
  }
</script>

<section class="relative flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header">
    <div class="flex items-center justify-between gap-3">
      <div class="flex min-w-0 items-center gap-2">
        <button
          type="button"
          class="workshop-rail-btn shrink-0"
          aria-label="Open sessions"
          title="Sessions"
          onclick={() => layout.toggleSessionDrawer()}
        >
          <PanelLeft size={16} strokeWidth={1.75} />
        </button>
        <div class="min-w-0">
          <h1 class="text-sm font-semibold text-surface-50">Chat</h1>
          <p class="truncate text-[11px] text-surface-400" title={chat.sessionId}>
            {sessionLabel}
          </p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          class="workshop-rail-btn"
          aria-label="Identity recall"
          title="Identity recall"
          onclick={() => layout.toggleIdentityDrawer()}
        >
          <Users size={16} strokeWidth={1.75} />
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
    {/if}
  </header>

  <div bind:this={scrollEl} class="flex-1 space-y-3 overflow-y-auto px-4 py-3">
    {#each chat.messages as message (message.id)}
      <article
        class="max-w-2xl {message.role === 'user'
          ? 'ml-auto rounded-md bg-primary-500/12 px-3 py-2'
          : message.role === 'system'
            ? 'workshop-faint px-1'
            : 'border-l-2 border-surface-500/35 pl-3'}"
      >
        {#if message.role !== "system"}
          <p class="mb-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] uppercase tracking-wide text-surface-500">
            <span>{message.role}</span>
            {#if message.role === "assistant"}
              {@const answer = formatAnswerState(message.answerState)}
              {#if answer}
                <span class="normal-case {answerStateTextClass(answer.tone)}">
                  · {answer.label}
                </span>
              {/if}
            {/if}
            {#if message.streaming && message.phase}
              <span class="normal-case text-primary-300">
                · {formatTurnPhase(message.phase)}
              </span>
            {:else if message.streaming}
              <span class="workshop-faint normal-case animate-pulse">· working</span>
            {/if}
          </p>
        {/if}

        {#if message.role === "assistant" && (message.tools?.length || message.statusLine || message.reasoning)}
          <div class="mb-1.5 space-y-1">
            {#if message.tools && message.tools.length > 0}
              <p class="font-mono text-[10px] text-surface-500">
                {message.tools.map((tool) => formatToolName(tool)).join(" · ")}
              </p>
            {/if}
            {#if message.streaming && message.statusLine}
              <p class="workshop-faint">{message.statusLine}</p>
            {/if}
            {#if message.reasoning?.trim()}
              <details class="workshop-faint">
                <summary class="cursor-pointer select-none text-surface-400 hover:text-surface-200">
                  Reasoning
                </summary>
                <p class="mt-1 whitespace-pre-wrap leading-relaxed text-surface-300">
                  {message.reasoning}
                </p>
              </details>
            {/if}
          </div>
        {/if}

        {#if message.role === "assistant"}
          <MarkdownContent content={message.content || "…"} />
        {:else}
          <p class="whitespace-pre-wrap text-sm leading-relaxed text-surface-100">
            {message.content}
          </p>
        {/if}
      </article>
    {:else}
      <div class="flex h-full min-h-[200px] flex-col justify-center">
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
      </div>
    {/each}
  </div>

  <form class="workshop-composer" onsubmit={submit}>
    <div class="flex items-end gap-2">
      <textarea
        class="textarea min-h-[36px] max-h-28 flex-1 resize-none py-1.5 text-sm"
        placeholder="Message · /ask for background jobs"
        rows="1"
        bind:value={chat.draft}
        disabled={chat.isStreaming}
        onkeydown={handleKeydown}
      ></textarea>
      <button
        type="submit"
        class="btn btn-sm variant-filled-primary h-8 w-8 shrink-0 p-0"
        disabled={chat.isStreaming || !chat.draft.trim()}
        aria-label="Send message"
      >
        {chat.isStreaming ? "…" : "↑"}
      </button>
    </div>
  </form>
</section>
