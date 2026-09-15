<script lang="ts">
  import { LoaderCircle, Mic, MicOff, PhoneOff } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { haptic } from "$lib/haptics";
  import {
    connectLiveVoice,
    disconnectLiveVoice,
    liveVoiceState,
    setLiveVoiceMuted,
  } from "$lib/liveVoice";

  let busy = $state(false);
  const canStart = $derived(Boolean(chat.sessionId.trim()) && !busy);
  const label = $derived.by(() => {
    if ($liveVoiceState.phase === "connecting") return "Connecting…";
    if ($liveVoiceState.phase === "speaking") return "Medousa is speaking";
    if ($liveVoiceState.phase === "thinking") return "Medousa is thinking";
    if ($liveVoiceState.phase === "muted") return "Microphone muted";
    if ($liveVoiceState.phase === "failed") return $liveVoiceState.error ?? "Live stopped";
    if ($liveVoiceState.active) return "Listening";
    return "Talk live";
  });

  async function start() {
    if (!canStart) return;
    busy = true;
    haptic("medium");
    try {
      await connectLiveVoice(workshops.activeLabel || "Medousa", chat.sessionId);
    } catch {
      haptic("warning");
    } finally {
      busy = false;
    }
  }

  async function toggleMuted() {
    if (busy || !$liveVoiceState.active) return;
    busy = true;
    haptic("light");
    try {
      await setLiveVoiceMuted(!$liveVoiceState.muted);
    } finally {
      busy = false;
    }
  }

  async function stop() {
    if (busy) return;
    busy = true;
    haptic("light");
    try {
      await disconnectLiveVoice();
    } finally {
      busy = false;
    }
  }
</script>

<div class="border-b border-white/6 bg-surface-950/95 px-3 pb-2 backdrop-blur-xl">
  <div class="flex min-h-11 items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-2 py-1.5">
    <button
      type="button"
      class="grid size-9 shrink-0 place-items-center rounded-full bg-fuchsia-500 text-white disabled:opacity-45"
      disabled={!canStart || $liveVoiceState.active}
      aria-label="Start Medousa Live"
      onclick={start}
    >
      {#if busy || $liveVoiceState.phase === "connecting"}
        <LoaderCircle class="size-4 animate-spin" />
      {:else}
        <Mic class="size-4" />
      {/if}
    </button>

    <div class="min-w-0 flex-1">
      <p class="truncate text-sm font-medium text-white">{label}</p>
      <p class="truncate text-[11px] text-white/45">
        {$liveVoiceState.active ? workshops.activeLabel : "Medousa Live · preview"}
      </p>
    </div>

    {#if $liveVoiceState.active}
      <button
        type="button"
        class="grid size-9 place-items-center rounded-full bg-white/8 text-white"
        aria-label={$liveVoiceState.muted ? "Unmute microphone" : "Mute microphone"}
        onclick={toggleMuted}
      >
        {#if $liveVoiceState.muted}<MicOff class="size-4" />{:else}<Mic class="size-4" />{/if}
      </button>
      <button
        type="button"
        class="grid size-9 place-items-center rounded-full bg-red-500/90 text-white"
        aria-label="End Medousa Live"
        onclick={stop}
      >
        <PhoneOff class="size-4" />
      </button>
    {/if}
  </div>
</div>
