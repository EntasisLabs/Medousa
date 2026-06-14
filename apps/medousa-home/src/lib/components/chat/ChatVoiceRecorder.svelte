<script lang="ts">
  import { Check, LoaderCircle, Plus, X } from "@lucide/svelte";
  import { formatVoiceElapsed } from "$lib/utils/composerMicMonitor";

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

  const hint = $derived(
    transcribing
      ? "Transcribing…"
      : busy
        ? "Finishing…"
        : micActive
          ? "Recording…"
          : "Starting microphone…",
  );
</script>

<div
  class="composer-voice-recorder {mobile ? 'composer-voice-recorder-mobile' : ''}"
  role="region"
  aria-label="Voice input"
>
  {#if onAttach}
    <button
      type="button"
      class="composer-bar-icon-btn"
      aria-label="Attach file"
      disabled={disabled || uploading}
      onclick={() => onAttach?.()}
    >
      {#if uploading}
        <LoaderCircle size={16} class="animate-spin" />
      {:else}
        <Plus size={18} strokeWidth={2} />
      {/if}
    </button>
  {/if}

  <div class="composer-voice-recorder-main">
    <div class="composer-voice-wave" aria-hidden="true">
      {#each levels as level, index (index)}
        <span
          class="composer-voice-wave-bar"
          style:height="{Math.round(8 + level * 22)}px"
          style:opacity="{0.35 + level * 0.65}"
        ></span>
      {/each}
    </div>

    <p class="composer-voice-preview composer-voice-preview-empty">{hint}</p>
  </div>

  <span class="composer-voice-timer" aria-live="polite">{formatVoiceElapsed(elapsed)}</span>

  <button
    type="button"
    class="composer-bar-icon-btn composer-voice-recorder-cancel"
    aria-label="Cancel voice input"
    disabled={busy}
    onclick={onCancel}
  >
    <X size={16} strokeWidth={2} />
  </button>

  <button
    type="button"
    class="composer-bar-icon-btn composer-voice-recorder-confirm"
    aria-label="Done recording"
    disabled={busy}
    onclick={onConfirm}
  >
    {#if busy}
      <LoaderCircle size={16} class="animate-spin" />
    {:else}
      <Check size={16} strokeWidth={2.5} />
    {/if}
  </button>
</div>
