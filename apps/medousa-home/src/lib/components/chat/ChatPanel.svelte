<script lang="ts">
  import { ExternalLink, PanelLeft } from "@lucide/svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import {
    sendInteractiveTurn,
    startInteractiveStream,
    stopInteractiveStream,
  } from "$lib/daemon";

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

  async function submit(event: Event) {
    event.preventDefault();
    const prompt = chat.draft.trim();
    if (!prompt || chat.isStreaming) return;

    chat.draft = "";
    chat.beginUserMessage(prompt);

    try {
      await stopInteractiveStream();
      const accepted = await sendInteractiveTurn(chat.sessionId, prompt);
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
  <header class="border-b border-surface-500/20 px-5 py-3">
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
          <h1 class="text-base font-semibold">Chat</h1>
          <p class="truncate text-xs text-surface-400" title={chat.sessionId}>
            {sessionLabel}
          </p>
        </div>
      </div>
      {#if showPopout && isTauri()}
        <button
          type="button"
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-container-token text-surface-400 transition hover:bg-surface-800/80 hover:text-surface-200"
          aria-label="Pop out chat"
          title="Pop out"
          onclick={() => showChatPopout()}
        >
          <ExternalLink size={20} strokeWidth={1.75} />
        </button>
      {/if}
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
          : 'rounded-container-token bg-surface-800/80 px-4 py-3'}"
      >
        <p class="mb-1 text-xs capitalize tracking-wide text-surface-400">
          {message.role}
          {#if message.streaming}
            <span class="ml-2 animate-pulse">Thinking…</span>
          {/if}
        </p>
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

  <form class="border-t border-surface-500/20 p-4" onsubmit={submit}>
    <div
      class="rounded-container-token border border-surface-500/25 bg-surface-900/80 p-3 shadow-sm"
    >
      <textarea
        class="textarea w-full min-h-[72px] border-0 bg-transparent p-0 focus:ring-0"
        placeholder="What do you want to work on?"
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
