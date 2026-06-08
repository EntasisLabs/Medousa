<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
  import { haptic } from "$lib/haptics";
  import { chat } from "$lib/stores/chat.svelte";
  import { createTurnTicket, startInteractiveStream } from "$lib/daemon";
  import { setMobileComposerFocus } from "$lib/utils/mobileKeyboardViewport";

  let composerBlurTimer: ReturnType<typeof setTimeout> | undefined;

  function parseDaemonAskPrompt(value: string): string | null {
    const trimmed = value.trim();
    if (trimmed.startsWith("/ask ")) return trimmed.slice(5).trim();
    if (trimmed.startsWith("/daemon ask ")) return trimmed.slice(12).trim();
    return null;
  }

  async function submitTurn(
    userContent: string,
    prompt: string,
    mode: "interactive" | "background",
  ) {
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
    haptic("medium");

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

  function handleComposerFocus() {
    if (composerBlurTimer) {
      clearTimeout(composerBlurTimer);
      composerBlurTimer = undefined;
    }
    setMobileComposerFocus(true);
    window.dispatchEvent(new CustomEvent("medousa-chat-composer-focus"));
  }

  function handleComposerBlur() {
    composerBlurTimer = setTimeout(() => {
      setMobileComposerFocus(false);
      composerBlurTimer = undefined;
    }, 150);
  }
</script>

<form class="mobile-chat-composer" onsubmit={submit}>
  <div class="composer-bar composer-bar-mobile">
    <GrowingTextarea
      bind:value={chat.draft}
      placeholder="Message"
      disabled={chat.composerBlocked}
      maxHeight={144}
      minHeight={34}
      onkeydown={handleKeydown}
      onfocus={handleComposerFocus}
      onblur={handleComposerBlur}
      aria-label="Message"
    />
    <button
      type="submit"
      class="composer-bar-send"
      disabled={chat.composerBlocked || !chat.draft.trim()}
      aria-label="Send message"
      onmousedown={(event) => event.preventDefault()}
    >
      {chat.composerBlocked ? "…" : "↑"}
    </button>
  </div>
</form>
