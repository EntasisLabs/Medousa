<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { LoaderCircle, Mic, Plus } from "@lucide/svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import ChatAttachmentChips from "$lib/components/chat/ChatAttachmentChips.svelte";
  import ChatModelPicker from "$lib/components/chat/ChatModelPicker.svelte";
  import ChatVoiceRecorder from "$lib/components/chat/ChatVoiceRecorder.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { haptic } from "$lib/haptics";
  import {
    idleVoiceWaveform,
    pushVoiceWaveSample,
    voiceWaveLevelFromMic,
  } from "$lib/utils/composerMicMonitor";
  import {
    composerMicSupported,
    startComposerAudioCapture,
    type ComposerAudioCaptureSession,
  } from "$lib/utils/composerAudioCapture";
  import {
    appendComposerDraft,
    composerSttStatus,
    transcribeComposerAudio,
  } from "$lib/utils/composerStt";

  interface Props {
    mobile?: boolean;
    disabled?: boolean;
    composerBlocked?: boolean;
    onkeydown?: (event: KeyboardEvent) => void;
    onfocus?: () => void;
    onblur?: () => void;
    onOpenVoiceSettings?: () => void;
  }

  let {
    mobile = false,
    disabled = false,
    composerBlocked = false,
    onkeydown,
    onfocus,
    onblur,
    onOpenVoiceSettings,
  }: Props = $props();

  let voiceActive = $state(false);
  let voiceError = $state<string | null>(null);
  let voiceBusy = $state(false);
  let voiceTranscribing = $state(false);
  let voiceElapsed = $state(0);
  let voiceLevels = $state(idleVoiceWaveform());
  let voiceMicActive = $state(false);
  let dictationBase = $state("");
  let sttAvailable = $state(false);
  let sttReason = $state<string | null>(null);

  let voiceSession: ComposerAudioCaptureSession | null = null;
  let waveFrame = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let voiceClosed = false;

  const micSupported = composerMicSupported();
  const voiceSupported = $derived(micSupported && sttAvailable);
  const voiceHint = $derived(
    !micSupported
      ? "Microphone capture unavailable"
      : sttReason ?? "Voice input unavailable",
  );
  const blocked = $derived(disabled || composerBlocked);
  const canSend = $derived(
    !blocked && (chat.draft.trim().length > 0 || chat.pendingMediaRefs.length > 0),
  );

  onMount(() => {
    void refreshSttStatus();
  });

  onDestroy(() => {
    releaseVoiceSession();
    stopWaveformAndTimer();
    voiceActive = false;
  });

  async function refreshSttStatus() {
    const status = await composerSttStatus();
    sttAvailable = status.available;
    sttReason = status.reason;
  }

  function tickWaveform() {
    if (!voiceActive || !voiceSession) return;
    const level = voiceWaveLevelFromMic(voiceSession.getLevel());
    voiceLevels = pushVoiceWaveSample(voiceLevels, level);
    waveFrame = requestAnimationFrame(tickWaveform);
  }

  function stopWaveformAndTimer() {
    if (waveFrame) cancelAnimationFrame(waveFrame);
    waveFrame = 0;
    if (elapsedTimer) clearInterval(elapsedTimer);
    elapsedTimer = null;
    voiceMicActive = false;
    voiceLevels = idleVoiceWaveform();
  }

  function releaseVoiceSession() {
    voiceSession?.abort();
    voiceSession = null;
  }

  function teardownVoice(options: { restoreDraft: boolean }) {
    releaseVoiceSession();
    stopWaveformAndTimer();
    if (options.restoreDraft) {
      chat.draft = dictationBase;
    }
    voiceActive = false;
    voiceBusy = false;
    voiceTranscribing = false;
    voiceElapsed = 0;
  }

  function closeVoice(options: { restoreDraft: boolean; commitText?: string }) {
    if (voiceClosed) return;
    voiceClosed = true;

    if (options.commitText?.trim()) {
      chat.draft = appendComposerDraft(dictationBase, options.commitText);
      if (mobile) haptic("medium");
    }

    teardownVoice({ restoreDraft: options.restoreDraft });
  }

  async function startVoice() {
    if (blocked || voiceBusy || voiceActive) return;
    await refreshSttStatus();
    if (!voiceSupported) return;

    voiceError = null;
    voiceBusy = true;
    voiceClosed = false;
    voiceTranscribing = false;
    dictationBase = chat.draft;
    voiceElapsed = 0;
    voiceLevels = idleVoiceWaveform();
    if (mobile) haptic("light");

    const nextSession = await startComposerAudioCapture({
      onError: (message) => {
        if (voiceClosed) return;
        voiceError = message;
        teardownVoice({ restoreDraft: true });
      },
    });

    if (!nextSession) {
      voiceError = voiceError ?? "Could not start microphone.";
      voiceBusy = false;
      return;
    }

    voiceSession = nextSession;
    voiceActive = true;
    voiceMicActive = true;
    voiceBusy = false;
    elapsedTimer = setInterval(() => {
      voiceElapsed += 1;
    }, 1000);
    waveFrame = requestAnimationFrame(tickWaveform);
  }

  function cancelVoice() {
    closeVoice({ restoreDraft: true });
    if (mobile) haptic("light");
  }

  async function confirmVoice() {
    if (!voiceActive || voiceClosed || voiceBusy || !voiceSession) return;

    voiceBusy = true;
    voiceTranscribing = true;
    stopWaveformAndTimer();

    const session = voiceSession;
    voiceSession = null;

    try {
      const { blob } = await session.stop();
      const text = await transcribeComposerAudio(blob);
      if (!text.trim()) {
        voiceError = "No speech detected — try again closer to the mic.";
        closeVoice({ restoreDraft: true });
        return;
      }
      closeVoice({ restoreDraft: false, commitText: text });
    } catch (err) {
      voiceError = err instanceof Error ? err.message : String(err);
      closeVoice({ restoreDraft: true });
    }
  }
</script>

<ChatAttachmentChips {disabled} />

{#if voiceError}
  <p class="composer-voice-status composer-voice-status-error" role="alert">{voiceError}</p>
{/if}

<div
  class="composer-bar chat-composer-shell {mobile ? 'composer-bar-mobile' : 'chat-composer-bar'}"
>
  {#if voiceActive}
    <ChatVoiceRecorder
      {mobile}
      disabled={blocked}
      uploading={chat.pendingMediaUploading}
      levels={voiceLevels}
      elapsed={voiceElapsed}
      transcribing={voiceTranscribing}
      micActive={voiceMicActive}
      busy={voiceBusy}
      onCancel={cancelVoice}
      onConfirm={() => void confirmVoice()}
      onAttach={() => void chat.attachFilesFromPicker()}
    />
  {:else}
    <button
      type="button"
      class="composer-bar-icon-btn"
      aria-label="Attach file"
      disabled={blocked || chat.pendingMediaUploading}
      onclick={() => void chat.attachFilesFromPicker()}
    >
      {#if chat.pendingMediaUploading}
        <LoaderCircle size={16} class="animate-spin" />
      {:else}
        <Plus size={18} strokeWidth={2} />
      {/if}
    </button>

    <ChatModelPicker {disabled} readonly={mobile} {onOpenVoiceSettings} />

    <GrowingTextarea
      bind:value={chat.draft}
      placeholder={mobile ? "Message" : "Message Medousa…"}
      disabled={blocked}
      maxHeight={mobile ? 144 : 128}
      minHeight={mobile ? 34 : 36}
      {onkeydown}
      {onfocus}
      {onblur}
      aria-label="Message"
    />

    <button
      type="button"
      class="composer-bar-icon-btn composer-bar-voice-btn"
      aria-label={voiceSupported ? "Voice input" : voiceHint}
      title={voiceSupported ? "Voice input" : voiceHint}
      disabled={blocked || !voiceSupported}
      onclick={() => void startVoice()}
    >
      <Mic size={16} strokeWidth={2} />
    </button>

    <button
      type="submit"
      class="composer-bar-send"
      disabled={!canSend}
      aria-label="Send message"
      onmousedown={(event) => event.preventDefault()}
    >
      {composerBlocked ? "…" : "↑"}
    </button>
  {/if}
</div>
