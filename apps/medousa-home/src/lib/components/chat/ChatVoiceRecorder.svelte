<script lang="ts">
  import { Check, LoaderCircle, Plus, X } from "@lucide/svelte";
  import { displayVoiceWaveform, formatVoiceElapsed } from "$lib/utils/composerMicMonitor";

  interface Props {
    mobile?: boolean;
    disabled?: boolean;
    uploading?: boolean;
    levels: number[];
    elapsed: number;
    transcribing?: boolean;
    busy?: boolean;
    micActive?: boolean;
    onCancel: () => void;
    onConfirm: () => void;
    onAttach?: () => void;
  }

  let {
    mobile = false,
    disabled = false,
    uploading = false,
    levels,
    elapsed,
    transcribing = false,
    busy = false,
    micActive = false,
    onCancel,
    onConfirm,
    onAttach,
  }: Props = $props();

  const waveBars = $derived(displayVoiceWaveform(levels));
  const phase = $derived(
    transcribing ? "transcribing" : busy ? "finishing" : micActive ? "recording" : "starting",
  );
  const phaseLabel = $derived(
    transcribing
      ? "Transcribing"
      : busy
        ? "Finishing"
        : micActive
          ? "Recording"
          : "Starting",
  );
</script>

<div
  class="composer-voice-recorder composer-voice-recorder-{phase} {mobile
    ? 'composer-voice-recorder-mobile'
    : ''}"
  role="region"
  aria-label="Voice input"
  aria-busy={busy || transcribing}
>
  {#if onAttach}
    <button
      type="button"
      class="composer-bar-icon-btn composer-voice-attach"
      aria-label="Attach file"
      disabled={disabled || uploading || busy}
      onclick={() => onAttach?.()}
    >
      {#if uploading}
        <LoaderCircle size={16} class="animate-spin" />
      {:else}
        <Plus size={18} strokeWidth={2} />
      {/if}
    </button>
  {/if}

  <div class="composer-voice-stage">
    <span class="composer-voice-live" aria-hidden="true">
      <span class="composer-voice-live-core"></span>
      <span class="composer-voice-live-ring"></span>
    </span>

    <div class="composer-voice-wave-wrap" aria-hidden="true">
      <div class="composer-voice-wave">
        {#each waveBars as level, index (index)}
          <span
            class="composer-voice-wave-bar"
            style:height="{Math.round(5 + level * 19)}px"
            style:opacity="{0.28 + level * 0.72}"
          ></span>
        {/each}
      </div>
    </div>

    <div class="composer-voice-meta">
      <span class="composer-voice-phase">{phaseLabel}</span>
      <span class="composer-voice-meta-dot" aria-hidden="true">·</span>
      <span class="composer-voice-timer" aria-live="polite">{formatVoiceElapsed(elapsed)}</span>
    </div>
  </div>

  <div class="composer-voice-actions">
    <button
      type="button"
      class="composer-voice-cancel"
      aria-label="Cancel voice input"
      disabled={busy}
      onclick={onCancel}
    >
      <X size={15} strokeWidth={2.25} />
    </button>

    <button
      type="button"
      class="composer-voice-done"
      aria-label={transcribing ? "Transcribing" : "Done recording"}
      disabled={busy}
      onclick={onConfirm}
    >
      {#if busy}
        <LoaderCircle size={16} class="animate-spin" />
      {:else}
        <Check size={16} strokeWidth={2.75} />
      {/if}
    </button>
  </div>
</div>
