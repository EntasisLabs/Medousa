<script lang="ts">
  import { ExternalLink, PanelLeft, Users } from "@lucide/svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
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

  import { answerStateBadgeClass, formatAnswerState } from "$lib/utils/formatAnswer";
  import { formatToolName, formatTurnPhase } from "$lib/utils/formatTurn";
  import { isTauri, showChatPopout } from "$lib/window";

  interface Props {
    visible: boolean;
    showPopout?: boolean;
  }

  let { visible, showPopout = true }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();

  const sessionLabel = $derived(chat.currentSessionLabel());

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
</script>

<section class="relative flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header py-3">
    <div class="flex items-start justify-between gap-3">
      <div class="flex min-w-0 items-start gap-3">
        <button
          type="button"
          class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-container-token text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-200"
          aria-label="Open sessions"
          title="Sessions"
          onclick={() => layout.toggleSessionDrawer()}
        >
          <PanelLeft size={20} strokeWidth={1.75} />
        </button>
        <div class="min-w-0">
          <h1 class="text-base font-semibold text-surface-50">Chat</h1>
          <p class="truncate text-xs text-surface-300" title={chat.sessionId}>
            {sessionLabel}
          </p>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-container-token text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-200"
          aria-label="Identity recall"
          title="Identity recall"
          onclick={() => layout.toggleIdentityDrawer()}
        >
          <Users size={20} strokeWidth={1.75} />
        </button>
        {#if showPopout && isTauri()}
          <button
            type="button"
            class="flex h-8 w-8 items-center justify-center rounded-container-token text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-200"
            aria-label="Pop out chat"
            title="Pop out"
            onclick={() => showChatPopout()}
          >
            <ExternalLink size={20} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
    </div>
    {#if chat.streamError}
      <p class="mt-1 text-xs text-error-400">{chat.streamError}</p>
    {/if}
  </header>

  <div bind:this={scrollEl} class="flex-1 space-y-4 overflow-y-auto px-5 py-4">
    {#each chat.messages as message (message.id)}
      <article
        class="max-w-3xl {message.role === 'user'
          ? 'ml-auto rounded-container-token bg-primary-500/15 px-4 py-3'
          : 'workshop-inset px-4 py-3'}"
      >
        <p class="workshop-label mb-1 flex flex-wrap items-center gap-2 capitalize tracking-wide">
          <span>{message.role}</span>
          {#if message.role === "assistant"}
            {@const answer = formatAnswerState(message.answerState)}
            {#if answer}
              <span class="badge {answerStateBadgeClass(answer.tone)} text-[10px] normal-case">
                {answer.label}
              </span>
            {/if}
          {/if}
          {#if message.streaming && message.phase}
            <span class="ml-2 text-primary-300">
              {formatTurnPhase(message.phase)}
            </span>
          {:else if message.streaming}
            <span class="workshop-faint ml-2 animate-pulse">Working…</span>
          {/if}
        </p>

        {#if message.role === "assistant" && (message.tools?.length || message.statusLine || message.reasoning)}
          <div class="mb-2 space-y-2">
            {#if message.tools && message.tools.length > 0}
              <div class="flex flex-wrap gap-1">
                {#each message.tools as tool (tool)}
                  <span class="badge variant-soft-primary text-[10px]">
                    {formatToolName(tool)}
                  </span>
                {/each}
              </div>
            {/if}
            {#if message.streaming && message.statusLine}
              <p class="workshop-faint">{message.statusLine}</p>
            {/if}
            {#if message.reasoning?.trim()}
              <details class="workshop-faint">
                <summary class="cursor-pointer select-none text-surface-300 hover:text-surface-100">
                  Reasoning
                </summary>
                <p class="mt-1 whitespace-pre-wrap leading-relaxed text-surface-200">
                  {message.reasoning}
                </p>
              </details>
            {/if}
          </div>
        {/if}

        {#if message.role === "assistant"}
          <MarkdownContent content={message.content || "…"} />
        {:else}
          <p class="whitespace-pre-wrap text-sm leading-relaxed">{message.content}</p>
        {/if}
      </article>
    {:else}
      <div class="flex h-full min-h-[280px] items-center justify-center">
        <EmptyState
          title="The workshop is quiet"
          description="Ask a question, delegate work, or open the library."
        />
      </div>
    {/each}
  </div>

  <form class="workshop-composer" onsubmit={submit}>
    <div class="workshop-inset p-3">
      <textarea
        class="textarea w-full min-h-[72px] border-0 bg-transparent p-0 focus:ring-0"
        placeholder="What do you want to work on? · /ask for background jobs"
        bind:value={chat.draft}
        disabled={chat.isStreaming}
        onkeydown={handleKeydown}
      ></textarea>
      <div class="mt-2 flex items-center justify-end gap-2 border-t border-surface-500/15 pt-2">
        <button
          type="submit"
          class="btn variant-filled-primary btn-sm"
          disabled={chat.isStreaming || !chat.draft.trim()}
          aria-label="Send message"
        >
          {chat.isStreaming ? "…" : "↑"}
        </button>
      </div>
    </div>
  </form>
</section>
