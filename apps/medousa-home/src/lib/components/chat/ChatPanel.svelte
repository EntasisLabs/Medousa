<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import {
    sendInteractiveTurn,
    startInteractiveStream,
    stopInteractiveStream,
  } from "$lib/daemon";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  let scrollEl: HTMLDivElement | undefined = $state();

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

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="border-b border-surface-500/20 px-5 py-3">
    <h1 class="text-base font-semibold">Chat</h1>
    <p class="text-xs text-surface-400 truncate">
      {chat.sessionId}
    </p>
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
        <p class="mb-1 text-xs uppercase tracking-wide text-surface-400">
          {message.role}
          {#if message.streaming}
            <span class="ml-2 animate-pulse">streaming…</span>
          {/if}
        </p>
        <p class="whitespace-pre-wrap text-sm leading-relaxed">{message.content}</p>
      </article>
    {:else}
      <div class="flex h-full min-h-[240px] flex-col items-center justify-center text-center">
        <p class="text-lg font-semibold tracking-wide text-surface-200">
          MEDOUSA HOME
        </p>
        <p class="mt-3 max-w-md text-sm text-surface-400">
          Ask a question, delegate work, or open the library. Turns stream from
          the daemon agent runtime.
        </p>
      </div>
    {/each}
  </div>

  <form class="border-t border-surface-500/20 p-4" onsubmit={submit}>
    <div
      class="rounded-container-token border border-surface-500/25 bg-surface-900/80 p-3 shadow-sm"
    >
      <textarea
        class="textarea w-full min-h-[72px] border-0 bg-transparent p-0 focus:ring-0"
        placeholder="Give Medousa a task…"
        bind:value={chat.draft}
        disabled={chat.isStreaming}
        onkeydown={handleKeydown}
      ></textarea>
      <div class="mt-2 flex items-center justify-between gap-2 border-t border-surface-500/15 pt-2">
        <div class="flex flex-wrap items-center gap-2 text-xs text-surface-400">
          <span class="badge variant-soft-surface">home</span>
          <span class="badge variant-soft-surface">balanced</span>
        </div>
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
