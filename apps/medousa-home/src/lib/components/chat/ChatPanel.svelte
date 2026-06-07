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
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="border-b border-surface-500/20 px-5 py-3">
    <h1 class="text-base font-semibold">Chat</h1>
    <p class="text-xs text-surface-400">Session {chat.sessionId}</p>
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
      <p class="text-sm text-surface-400">
        Talk to Medousa — turns stream from the daemon agent runtime.
      </p>
    {/each}
  </div>

  <form
    class="border-t border-surface-500/20 p-4"
    onsubmit={submit}
  >
    <div class="flex gap-2">
      <textarea
        class="textarea flex-1 min-h-[72px]"
        placeholder="Ask Medousa…"
        bind:value={chat.draft}
        disabled={chat.isStreaming}
      ></textarea>
      <button
        type="submit"
        class="btn variant-filled-primary self-end"
        disabled={chat.isStreaming || !chat.draft.trim()}
      >
        Send
      </button>
    </div>
  </form>
</section>
