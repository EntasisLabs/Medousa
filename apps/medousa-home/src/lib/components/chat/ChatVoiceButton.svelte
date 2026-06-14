<script lang="ts">
  import { onDestroy } from "svelte";
  import { Mic } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { haptic } from "$lib/haptics";
  import {
    appendComposerDraft,
    composerSpeechSupported,
    startComposerSpeech,
    type ComposerSpeechSession,
  } from "$lib/utils/composerSpeechInput";

  interface Props {
    disabled?: boolean;
    mobile?: boolean;
    onStatus?: (message: string | null) => void;
  }

  let { disabled = false, mobile = false, onStatus }: Props = $props();

  let listening = $state(false);
  let session: ComposerSpeechSession | null = $state(null);
  let errorMessage = $state<string | null>(null);
  let busy = $state(false);

  const supported = composerSpeechSupported();
  const blocked = $derived(disabled || !supported);

  $effect(() => {
    onStatus?.(
      errorMessage ??
        (listening ? "Listening… speak now, tap mic again to stop." : null),
    );
  });

  onDestroy(() => {
    session?.abort();
    session = null;
  });

  async function toggleListening() {
    if (listening) {
      stopListening();
      return;
    }
    if (blocked || busy) return;

    errorMessage = null;
    busy = true;
    if (mobile) haptic("light");

    const nextSession = await startComposerSpeech({
      onFinal: (text) => {
        chat.draft = appendComposerDraft(chat.draft, text);
        if (mobile) haptic("medium");
      },
      onEnd: () => {
        listening = false;
        session = null;
        busy = false;
      },
      onError: (message) => {
        errorMessage = message;
        listening = false;
        session = null;
        busy = false;
      },
    });

    busy = false;

    if (!nextSession) {
      if (!errorMessage) {
        errorMessage = supported
          ? "Speech input failed to start"
          : "Voice input is not available in this shell — try Safari or rebuild the app after granting mic permissions.";
      }
      return;
    }

    session = nextSession;
    listening = true;
  }

  function stopListening() {
    session?.stop();
    session = null;
    listening = false;
    busy = false;
    if (mobile) haptic("light");
  }
</script>

<button
  type="button"
  class="composer-bar-icon-btn composer-bar-voice-btn {listening ? 'composer-bar-voice-btn-active' : ''}"
  aria-label={listening ? "Stop voice input" : supported ? "Voice input" : "Voice input unavailable"}
  title={errorMessage ??
    (listening
      ? "Listening… tap to stop"
      : supported
        ? "Voice input"
        : "Voice input unavailable — rebuild app with mic permissions")}
  disabled={(blocked && !listening) || busy}
  onclick={() => void toggleListening()}
>
  <Mic size={16} strokeWidth={2} />
</button>
