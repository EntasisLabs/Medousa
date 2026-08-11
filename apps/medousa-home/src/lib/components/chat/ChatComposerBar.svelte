<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Mic, Square } from "@lucide/svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import ChatAttachmentChips from "$lib/components/chat/ChatAttachmentChips.svelte";
  import ChatModelPicker from "$lib/components/chat/ChatModelPicker.svelte";
  import ChatVoiceRecorder from "$lib/components/chat/ChatVoiceRecorder.svelte";
  import ComposerAgentChip from "$lib/components/chat/ComposerAgentChip.svelte";
  import ComposerPlusMenu from "$lib/components/chat/ComposerPlusMenu.svelte";
  import ContextUsageIndicator from "$lib/components/chat/ContextUsageIndicator.svelte";
  import MobileComposerTurnSettings from "$lib/components/mobile/MobileComposerTurnSettings.svelte";
  import ProfileSwitcherCompact from "$lib/components/mobile/ProfileSwitcherCompact.svelte";
  import WorkshopSwitcherCompact from "$lib/components/workshops/WorkshopSwitcherCompact.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { isTauri, isTauriMobilePlatform } from "$lib/platform";
  import { haptic } from "$lib/haptics";
  import type { AgentSessionConfigOption } from "$lib/daemon";
  import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";
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
    /** Hide attachment hint + model picker (Presence empty landing). */
    quietChrome?: boolean;
    /** Expose the source-aware model selector in this composer. */
    modelPickerEnabled?: boolean;
    agentRuntime?: ChatAgentRuntime;
    agentConfigOptions?: AgentSessionConfigOption[];
    agentRuntimePending?: boolean;
    onAgentRuntimeChange?: (runtime: ChatAgentRuntime) => void;
    onAgentConfigChange?: (configId: string, value: unknown) => void | Promise<void>;
    onkeydown?: (event: KeyboardEvent) => void;
    onfocus?: () => void;
    onblur?: () => void;
    onCursorChange?: (cursor: number) => void;
    /** Bound textarea for slash-menu placement. */
    element?: HTMLTextAreaElement | null;
  }

  let {
    mobile = false,
    disabled = false,
    composerBlocked = false,
    quietChrome = false,
    modelPickerEnabled = true,
    agentRuntime = "medousa",
    agentConfigOptions = [],
    agentRuntimePending = false,
    onAgentRuntimeChange,
    onAgentConfigChange,
    onkeydown,
    onfocus,
    onblur,
    onCursorChange,
    element = $bindable<HTMLTextAreaElement | null>(null),
  }: Props = $props();

  const showModelPicker = $derived(
    !quietChrome &&
      modelPickerEnabled &&
      (settings.showChatModelPicker || onAgentRuntimeChange !== undefined),
  );
  const placeholder = $derived(
    chat.hasWorkshopHandoff()
      ? "Steer the handoff…"
      : quietChrome
        ? "Ask anything"
        : "Message Medousa… Type / for skills",
  );

  function syncCursor() {
    onCursorChange?.(element?.selectionStart ?? chat.draft.length);
  }

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

  let plusAnchorEl = $state<HTMLElement | null>(null);
  let profileOpen = $state(false);
  let agentOpen = $state(false);
  let workshopOpen = $state(false);
  let stoppingTurn = $state(false);
  let dropActive = $state(false);
  let dropTargetEl = $state<HTMLDivElement | null>(null);

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
  const blocked = $derived(disabled || composerBlocked || runtime.savingControls);
  const canSend = $derived(
    !blocked && (chat.draft.trim().length > 0 || chat.pendingMediaRefs.length > 0),
  );

  onMount(() => {
    void refreshSttStatus();

    if (!isTauri() || isTauriMobilePlatform()) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void (async () => {
      const [{ getCurrentWebview }, { getCurrentWindow }] = await Promise.all([
        import("@tauri-apps/api/webview"),
        import("@tauri-apps/api/window"),
      ]);
      const scaleFactor = await getCurrentWindow().scaleFactor();
      if (disposed) return;

      unlisten = await getCurrentWebview().onDragDropEvent(({ payload }) => {
        if (payload.type === "leave") {
          dropActive = false;
          return;
        }

        const rect = dropTargetEl?.getBoundingClientRect();
        const x = payload.position.x / scaleFactor;
        const y = payload.position.y / scaleFactor;
        const inside = Boolean(
          rect &&
            rect.width > 0 &&
            rect.height > 0 &&
            x >= rect.left &&
            x <= rect.right &&
            y >= rect.top &&
            y <= rect.bottom,
        );

        if (payload.type === "drop") {
          dropActive = false;
          if (inside && !blocked && payload.paths.length > 0) {
            void chat.attachDroppedPaths(payload.paths);
          }
          return;
        }

        dropActive = inside && !blocked;
      });

      if (disposed) {
        unlisten();
        unlisten = null;
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
    };
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

  function carriesFiles(event: DragEvent): boolean {
    return Array.from(event.dataTransfer?.types ?? []).includes("Files");
  }

  function handleDragEnter(event: DragEvent) {
    if (blocked || !carriesFiles(event)) return;
    event.preventDefault();
    event.stopPropagation();
    dropActive = true;
  }

  function handleDragOver(event: DragEvent) {
    if (blocked || !carriesFiles(event)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    dropActive = true;
  }

  function handleDragLeave(event: DragEvent) {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget instanceof Node) {
      if (event.currentTarget.contains(next)) return;
    }
    dropActive = false;
  }

  function handleDrop(event: DragEvent) {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    event.stopPropagation();
    dropActive = false;
    if (blocked) return;
    const files = Array.from(event.dataTransfer?.files ?? []);
    if (files.length > 0) void chat.attachDroppedFiles(files);
  }

  async function stopActiveTurn() {
    if (stoppingTurn) return;
    stoppingTurn = true;
    haptic("medium");
    try {
      await chat.cancelActiveTurn();
    } finally {
      stoppingTurn = false;
    }
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

<div
  bind:this={dropTargetEl}
  class="chat-composer-drop-target"
  class:chat-composer-drop-target-active={dropActive}
  role="group"
  aria-label="Message composer; drop files to attach"
  ondragenter={handleDragEnter}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
{#if dropActive}
  <div class="chat-composer-drop-overlay" aria-hidden="true">
    <span>Drop to attach</span>
  </div>
{/if}

<ChatAttachmentChips {disabled} />

{#if voiceError}
  <p class="composer-voice-status composer-voice-status-error" role="alert">{voiceError}</p>
{/if}

{#if mobile}
  <div
    class="mobile-composer-dock {voiceActive ? 'mobile-composer-dock-voice' : ''} {voiceTranscribing
      ? 'composer-bar-voice-transcribing'
      : ''}"
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
      <GrowingTextarea
        bind:value={chat.draft}
        bind:element
        placeholder="Message… / for skills"
        disabled={blocked}
        maxHeight={360}
        minHeight={34}
        class="mobile-composer-dock-input"
        {onkeydown}
        {onfocus}
        {onblur}
        oninput={syncCursor}
        onclick={syncCursor}
        onkeyup={syncCursor}
        onselect={syncCursor}
        aria-label="Message"
      />

      <div class="mobile-composer-dock-toolbar">
        <div bind:this={plusAnchorEl} class="composer-plus-anchor relative shrink-0">
          <ComposerPlusMenu
            disabled={blocked}
            showWorkshop={true}
            onProfile={() => {
              agentOpen = false;
              profileOpen = true;
            }}
            onAgent={() => {
              profileOpen = false;
              agentOpen = true;
            }}
            onWorkshop={() => {
              profileOpen = false;
              agentOpen = false;
              workshopOpen = true;
            }}
          />
        </div>

        <ProfileSwitcherCompact
          showChip
          bind:open={profileOpen}
          anchorEl={plusAnchorEl}
        />
        <ComposerAgentChip showChip bind:open={agentOpen} anchorEl={plusAnchorEl} />
        <WorkshopSwitcherCompact
          variant="mobile"
          hideWhenSingle={false}
          showTrigger={false}
          bind:sheetOpen={workshopOpen}
        />

        {#if showModelPicker}
          {#if isTauriMobilePlatform()}
            <MobileComposerTurnSettings disabled={blocked} quiet />
          {:else}
            <ChatModelPicker
              disabled={blocked}
              quiet
              {agentRuntime}
              {agentConfigOptions}
              {agentRuntimePending}
              onAgentConfigChange={onAgentConfigChange}
            />
          {/if}
        {/if}

        <span class="mobile-composer-dock-spacer" aria-hidden="true"></span>

        <ContextUsageIndicator compact />

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

        {#if chat.liveStreamActive}
          <button
            type="button"
            class="composer-bar-send composer-bar-stop"
            disabled={stoppingTurn}
            aria-label="Stop current turn"
            title="Stop current turn"
            onmousedown={(event) => event.preventDefault()}
            onclick={() => void stopActiveTurn()}
          >
            <Square size={12} strokeWidth={2.4} fill="currentColor" />
          </button>
        {/if}

        <button
          type="submit"
          class="composer-bar-send"
          disabled={!canSend}
          aria-label="Send message"
          onmousedown={(event) => event.preventDefault()}
        >
          {composerBlocked ? "…" : "↑"}
        </button>
      </div>
    {/if}
  </div>
{:else}
<div
  class="composer-bar chat-composer-shell chat-composer-bar composer-bar-stacked {voiceActive
    ? 'composer-bar-voice-mode'
    : ''} {voiceTranscribing ? 'composer-bar-voice-transcribing' : ''}"
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
    <GrowingTextarea
      bind:value={chat.draft}
      bind:element
      placeholder={placeholder}
      disabled={blocked}
      maxHeight={400}
      minHeight={36}
      class="composer-bar-stacked-input"
      {onkeydown}
      {onfocus}
      {onblur}
      oninput={syncCursor}
      onclick={syncCursor}
      onkeyup={syncCursor}
      onselect={syncCursor}
      aria-label={chat.hasWorkshopHandoff() ? "Steer handoff" : "Message"}
    />

    <div class="composer-bar-footer">
      <div bind:this={plusAnchorEl} class="composer-plus-anchor relative shrink-0">
        <ComposerPlusMenu
          disabled={blocked}
          onProfile={() => {
            agentOpen = false;
            profileOpen = true;
          }}
          onAgent={() => {
            profileOpen = false;
            agentOpen = true;
          }}
        />
      </div>

      <ProfileSwitcherCompact
        showChip
        bind:open={profileOpen}
        anchorEl={plusAnchorEl}
      />
      <ComposerAgentChip showChip bind:open={agentOpen} anchorEl={plusAnchorEl} />

      {#if showModelPicker}
        <ChatModelPicker
          disabled={blocked}
          quiet
          {agentRuntime}
          {agentConfigOptions}
          {agentRuntimePending}
          onAgentConfigChange={onAgentConfigChange}
        />
      {/if}

      <span class="composer-bar-footer-spacer" aria-hidden="true"></span>

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

      <ContextUsageIndicator />

      {#if chat.liveStreamActive}
        <button
          type="button"
          class="composer-bar-send composer-bar-stop"
          disabled={stoppingTurn}
          aria-label="Stop current turn"
          title="Stop current turn"
          onmousedown={(event) => event.preventDefault()}
          onclick={() => void stopActiveTurn()}
        >
          <Square size={10} strokeWidth={2.4} fill="currentColor" />
        </button>
      {/if}

      <button
        type="submit"
        class="composer-bar-send"
        disabled={!canSend}
        aria-label="Send message"
        onmousedown={(event) => event.preventDefault()}
      >
        {composerBlocked ? "…" : "↑"}
      </button>
    </div>
  {/if}
</div>
{/if}
</div>
