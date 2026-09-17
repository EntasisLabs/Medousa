<script lang="ts">
  import { LoaderCircle, Mic, MicOff, PhoneOff, MessageCircle } from "@lucide/svelte";
  import { switchMobileTab } from "$lib/mobileNavigation";
  import { chat } from "$lib/stores/chat.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { haptic } from "$lib/haptics";
  import { submitChatTurn } from "$lib/chat/submitTurnController";
  import { listSessionTurns } from "$lib/daemon";
  import { turnCompletionLedger } from "$lib/chat/turnCompletionLedger";
  import type { LiveTranscriptEntry } from "$lib/liveVoice";
  import { liveWorkSnapshot, settledLiveWorkSnapshot, waitForLiveWork } from "$lib/liveWorkResult";
  import {
    connectLiveVoice,
    disconnectLiveVoice,
    liveVoiceState,
    hearLatestLiveResult,
    setLiveVoiceMuted,
  } from "$lib/liveVoice";

  let busy = $state(false);
  let localError = $state<string | null>(null);
  let ownerEpoch = $state<number | null>(null);

  $effect(() => {
    const startRequested = () => { void start(); };
    window.addEventListener("medousa-live-start", startRequested);
    return () => window.removeEventListener("medousa-live-start", startRequested);
  });

  $effect(() => {
    if (ownerEpoch !== null && chat.workshopEpoch !== ownerEpoch) {
      ownerEpoch = null;
      void disconnectLiveVoice();
      localError = "Live ended because the workshop changed.";
    }
  });

  async function returnToConversation() {
    const sessionId = $liveVoiceState.sessionId;
    if (!sessionId) return;
    switchMobileTab("chat");
    if (chat.sessionId !== sessionId) await chat.switchSession(sessionId);
  }
  const canStart = $derived(!busy && !$liveVoiceState.active);
  const latestCaption = $derived($liveVoiceState.transcript.at(-1));
  const label = $derived.by(() => {
    if ($liveVoiceState.phase === "connecting") return "Connecting…";
    if ($liveVoiceState.error) return $liveVoiceState.error;
    if ($liveVoiceState.workStatus) return $liveVoiceState.workStatus;
    if ($liveVoiceState.phase === "speaking") return "Medousa is speaking";
    if ($liveVoiceState.phase === "thinking") return "Medousa is thinking";
    if ($liveVoiceState.phase === "muted") return "Microphone muted";
    if (localError) return localError;
    if ($liveVoiceState.error) return $liveVoiceState.error;
    if ($liveVoiceState.phase === "failed") return $liveVoiceState.error ?? "Live stopped";
    if ($liveVoiceState.active) return "Listening";
    return "Talk live";
  });

  async function start(protocol: "live" | "realtime" = "live") {
    if (!canStart) return;
    busy = true;
    localError = null;
    haptic("medium");
    try {
      if (!chat.sessionId.trim()) await chat.newSession();
      const sessionId = chat.sessionId.trim();
      if (!sessionId) throw new Error("Could not create a conversation for Medousa Live");
      const workshopEpoch = chat.workshopEpoch;
      ownerEpoch = workshopEpoch;
      await connectLiveVoice(workshops.activeLabel || "Medousa", sessionId, async (request, transcript, signal) => {
        if (chat.sessionId !== sessionId || chat.workshopEpoch !== workshopEpoch) {
          throw new Error("Return to the conversation where Live started before requesting work.");
        }
        return handoffToMedousa(request, transcript, signal);
      }, async () => {
        if (chat.sessionId === sessionId && chat.workshopEpoch === workshopEpoch && !chat.isStreaming) {
          await chat.reloadCurrentSession({ notice: false });
        }
      }, protocol);
    } catch (error) {
      ownerEpoch = null;
      localError = error instanceof Error ? error.message : String(error);
      haptic("warning");
    } finally {
      busy = false;
    }
  }

  async function handoffToMedousa(request: string, _transcript: LiveTranscriptEntry[], signal: AbortSignal) {
    if (chat.isStreaming) throw new Error("A turn is already running in this conversation. Let it finish first.");
    const sessionId = chat.sessionId;
    const workshopEpoch = chat.workshopEpoch;
    const workshopScope = chat.workshopScopeId ?? "";
    let turnId = "";
    let assistantId: string | null = null;
    await submitChatTurn({
      userContent: request,
      prompt: request,
      mode: "interactive",
      responseVoiceAppendix: "This answer returns to an ongoing Medousa voice conversation. Lead with a brief, complete spoken summary of the verified result and status, then any useful details. Do not announce internal routing. Do not repeat previously completed actions.",
      synchronizeAgentSession: async () => null,
      onAgentSessionLost: () => undefined,
      scrollToLatest: () => undefined,
      onAccepted: (ticket) => {
        turnId = ticket.turn_id;
        assistantId = chat.turns.get(turnId)?.messageId ?? null;
      },
    });
    haptic("success");
    if (!turnId) throw new Error("Medousa did not return a turn identity.");
    let lastPoll = 0;
    return waitForLiveWork(turnId, async () => {
      if (chat.workshopEpoch !== workshopEpoch) return null;
      const receipt = turnCompletionLedger.read(workshopScope, sessionId, turnId);
      if (receipt) return receipt;
      const turns = chat.sessionId === sessionId ? chat.turns : chat.sessionRuntimes.get(sessionId)?.turns;
      const turn = turns?.get(turnId);
      const messages = chat.messagesFor(sessionId);
      const active = liveWorkSnapshot(turn, messages);
      if (active?.terminal) return active;
      if (Date.now() - lastPoll < 1000) return active;
      lastPoll = Date.now();
      const records = await listSessionTurns(sessionId, false);
      if (signal.aborted || chat.workshopEpoch !== workshopEpoch) return null;
      const record = records.turns.find((item) => item.turn_id === turnId);
      if (!record || !["done", "error", "cancelled"].includes(record.phase)) return active;
      const message = chat.messagesFor(sessionId).find((item) =>
        (item.turnId === turnId || item.id === assistantId) && item.role === "assistant");
      return settledLiveWorkSnapshot(record, message) ?? active;
    }, signal);
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
      ownerEpoch = null;
    } finally {
      busy = false;
    }
  }
</script>

{#if $liveVoiceState.active || $liveVoiceState.phase === "connecting" || localError || $liveVoiceState.phase === "failed"}
<div class="shrink-0 bg-surface-950/95 px-3 pt-2 pb-1 backdrop-blur-xl">
  {#if !$liveVoiceState.active}
    <button
      type="button"
      class="flex min-h-11 w-full items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-2 py-1.5 text-left disabled:opacity-45"
      disabled={!canStart}
      aria-label="Start Medousa Live"
      onclick={() => void start()}
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
    {#if localError || $liveVoiceState.phase === "failed"}
      <button type="button" class="min-h-11 w-full text-xs text-white/60" disabled={!canStart}
        onclick={() => void start("realtime")}>Try legacy voice</button>
    {/if}
  {:else}
    <div class="rounded-2xl border border-white/10 bg-white/5 px-2 py-1.5">
      <div class="flex min-h-11 items-center gap-1">
        <span class={`mx-2 size-2 shrink-0 rounded-full ${$liveVoiceState.muted ? "bg-white/40" : "bg-fuchsia-400"}`}></span>
        <div class="min-w-0 flex-1">
          <p class="truncate text-xs font-medium text-white">{label}</p>
          <p class="truncate text-[10px] text-white/45">{$liveVoiceState.workshopName} · Live</p>
        </div>
        <button
          type="button"
          class="grid size-11 shrink-0 place-items-center rounded-full text-white/65"
          aria-label="Return to Live conversation"
          onclick={() => void returnToConversation()}
        >
          <MessageCircle class="size-4" />
        </button>
        <button
          type="button"
          class="grid size-11 shrink-0 place-items-center rounded-full bg-white/5 text-white"
          aria-label={$liveVoiceState.muted ? "Unmute microphone" : "Mute microphone"}
          onclick={toggleMuted}
        >
          {#if $liveVoiceState.muted}<MicOff class="size-4" />{:else}<Mic class="size-4" />{/if}
        </button>
        <button
          type="button"
          class="grid size-11 shrink-0 place-items-center rounded-full bg-red-500/10 text-red-400"
          aria-label="End Medousa Live"
          onclick={stop}
        >
          <PhoneOff class="size-4" />
        </button>
      </div>
      {#if latestCaption}
        <p class="truncate px-2 pb-1 text-[11px] text-white/50">
          {latestCaption.role === "user" ? "You" : "Medousa"}: {latestCaption.text}
        </p>
      {/if}
      {#if $liveVoiceState.resultAvailable}
        <button type="button" class="min-h-11 w-full text-xs text-white/70"
          onclick={() => hearLatestLiveResult()}>Hear result</button>
      {/if}
    </div>
  {/if}
</div>
{/if}
