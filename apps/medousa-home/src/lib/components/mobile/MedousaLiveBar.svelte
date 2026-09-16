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
  let localError = $state<string | null>(null);
  const canStart = $derived(!busy && !$liveVoiceState.active);
  const label = $derived.by(() => {
    if ($liveVoiceState.phase === "connecting") return "Connecting…";
    if ($liveVoiceState.phase === "speaking") return "Medousa is speaking";
    if ($liveVoiceState.phase === "thinking") return "Medousa is thinking";
    if ($liveVoiceState.phase === "muted") return "Microphone muted";
    if (localError) return localError;
    if ($liveVoiceState.phase === "failed") return $liveVoiceState.error ?? "Live stopped";
    if ($liveVoiceState.active) return "Listening";
    return "Talk live";
  });

  async function start() {
    if (!canStart) return;
    busy = true;
    localError = null;
    haptic("medium");
    try {
      if (!chat.sessionId.trim()) await chat.newSession();
      const sessionId = chat.sessionId.trim();
      if (!sessionId) throw new Error("Could not create a conversation for Medousa Live");
      await connectLiveVoice(workshops.activeLabel || "Medousa", sessionId);
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error);
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
  {#if !$liveVoiceState.active}
    <button
      type="button"
      class="flex min-h-11 w-full items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-2 py-1.5 text-left disabled:opacity-45"
      disabled={!canStart}
      aria-label="Start Medousa Live"
      onclick={start}
    >
      <span class="grid size-9 shrink-0 place-items-center rounded-full bg-fuchsia-500 text-white">
        {#if busy || $liveVoiceState.phase === "connecting"}
          <LoaderCircle class="size-4 animate-spin" />
        {:else}
          <Mic class="size-4" />
        {/if}
      </span>
      <span class="min-w-0 flex-1">
        <span class="block truncate text-sm font-medium text-white">{label}</span>
        <span class="block truncate text-[11px] text-white/45">Medousa Live · preview</span>
      </span>
    </button>
  {:else}
    <div class="flex min-h-11 items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-2 py-1.5">
      <span class="grid size-9 shrink-0 place-items-center rounded-full bg-fuchsia-500 text-white">
        <Mic class="size-4" />
      </span>
      <div class="min-w-0 flex-1">
        <p class="truncate text-sm font-medium text-white">{label}</p>
        <p class="truncate text-[11px] text-white/45">{workshops.activeLabel}</p>
      </div>
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
    </div>
  {/if}
</div>
