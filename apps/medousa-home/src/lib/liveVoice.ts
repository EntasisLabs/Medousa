import { invoke } from "@tauri-apps/api/core";
import { isTauriIos } from "$lib/platform";
import { get, writable } from "svelte/store";
import { liveWorkResultEvents, type LiveWorkResult } from "$lib/liveWorkResult";
import { LiveTimeline, liveDelegation, liveDelegationResult, liveTranscriptSlices } from "$lib/liveProtocol";
import { LiveDelegationCoordinator } from "$lib/liveDelegationCoordinator";
import { nativeLivePreviewEnabled } from "$lib/config/liveVoicePreferences";

export type LiveVoicePhase =
  | "idle"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking"
  | "muted"
  | "failed";

export interface LiveVoiceStatus {
  available: boolean;
  active: boolean;
  muted: boolean;
  phase: LiveVoicePhase;
  workshopName?: string | null;
  sessionId?: string | null;
  liveSessionId?: string | null;
  error?: string | null;
}

export interface LiveSessionAnswer {
  protocol?: "live" | "realtime";
  liveSessionId: string;
  sdp: string;
  seedHistory?: Array<{ role: "user" | "assistant"; content: string }>;
}

export function liveHistoryEvents(history: NonNullable<LiveSessionAnswer["seedHistory"]>) {
  return history.map((message) => ({
    type: "conversation.item.create",
    item: {
      type: "message",
      role: message.role,
      content: [{ type: message.role === "user" ? "input_text" : "output_text", text: message.content }],
    },
  }));
}

export interface LiveVoiceClientState extends LiveVoiceStatus {
  resultAvailable?: boolean;
  workStatus?: string | null;
  liveSessionId?: string | null;
  transcript: LiveTranscriptEntry[];
}

export interface LiveTranscriptEntry {
  id: string;
  role: "user" | "assistant";
  text: string;
}

export type LiveHandoffHandler = (
  request: string,
  transcript: LiveTranscriptEntry[],
  signal: AbortSignal,
  onAccepted?: (turnId: string) => void,
) => Promise<LiveWorkResult>;

export async function createLiveSession(
  sdp: string,
  sessionId: string,
  workshopName: string,
  protocol: "live" | "realtime" = "live",
): Promise<LiveSessionAnswer> {
  return invoke<LiveSessionAnswer>("live_voice_create_session", { sdp, sessionId, workshopName, protocol });
}

const unavailable: LiveVoiceStatus = {
  available: false,
  active: false,
  muted: false,
  phase: "idle",
  error: "Medousa Live voice sessions require the iOS app.",
};

const idle: LiveVoiceClientState = {
  available: isTauriIos(),
  active: false,
  muted: false,
  phase: "idle",
  transcript: [],
};

export const liveVoiceState = writable<LiveVoiceClientState>(idle);
export const liveVoiceUsage = writable<{ seconds: number | null; confirmed: boolean }>({ seconds: null, confirmed: false });

let peer: RTCPeerConnection | null = null;
let localStream: MediaStream | null = null;
let remoteAudio: HTMLAudioElement | null = null;
let dataChannel: RTCDataChannel | null = null;
let handoffHandler: LiveHandoffHandler | null = null;
let workAbort = new AbortController();
let responseActive = false;
const pendingNarrations: Record<string, unknown>[] = [];
const handledToolCalls = new Set<string>();
let transcriptWrites: Promise<void> = Promise.resolve();
let transcriptSavedHandler: (() => Promise<void>) | null = null;
let protocol: "live" | "realtime" = "realtime";
let timeline = new LiveTimeline();
let sessionReady = false;
let sessionFinalized: (() => void) | null = null;
let lastDelegationOffset = -1;
const delegatedRequests = new Set<string>();
const resultAcks = new Map<string, number>();
let coordinator = new LiveDelegationCoordinator();
let latestResult: { id: string; result: LiveWorkResult } | null = null;
let resultPresentation = 0;
let transcriptBindings: Array<{ start: number; turnId: string }> = [];
let transportMode: "webrtc" | "native" = "webrtc";
let nativeStatusTimer: number | null = null;
let transportGeneration = 0;
let nativePollActive = false;

interface NativeLiveEventEnvelope {
  sequence: number;
  event: Record<string, unknown>;
}

function persistTranscript(entry: LiveTranscriptEntry, attachment = false, targetTurnId?: string) {
  const { sessionId, liveSessionId } = get(liveVoiceState);
  const onSaved = transcriptSavedHandler;
  if (!sessionId || !liveSessionId) return;
  // Capture the origin before queuing; switching chats must never redirect writes.
  transcriptWrites = transcriptWrites.then(async () => {
    await invoke("live_voice_append_transcript", {
      sessionId, liveSessionId, itemId: entry.id, role: entry.role, text: entry.text, attachment, targetTurnId,
    });
    await onSaved?.();
  }).catch((error) => {
    updateClientState({ error: `Could not save Live message: ${String(error)}` });
  });
}

function updateClientState(next: Partial<LiveVoiceClientState>) {
  liveVoiceState.update((current) => ({ ...current, ...next }));
}

function closeMediaTransport() {
  transportGeneration += 1;
  workAbort.abort();
  sessionReady = false;
  for (const timer of resultAcks.values()) window.clearTimeout(timer);
  resultAcks.clear();
  responseActive = false;
  pendingNarrations.length = 0;
  dataChannel?.close();
  dataChannel = null;
  handoffHandler = null;
  transcriptSavedHandler = null;
  handledToolCalls.clear();
  peer?.close();
  peer = null;
  for (const track of localStream?.getTracks() ?? []) track.stop();
  localStream = null;
  if (nativeStatusTimer !== null) {
    window.clearInterval(nativeStatusTimer);
    nativeStatusTimer = null;
  }
  if (remoteAudio) {
    remoteAudio.pause();
    remoteAudio.srcObject = null;
    remoteAudio.remove();
    remoteAudio = null;
  }
}

function canSendLiveEvent(generation: number): boolean {
  if (workAbort.signal.aborted || generation !== transportGeneration) return false;
  if (transportMode === "native") return get(liveVoiceState).active;
  return dataChannel?.readyState === "open";
}

async function pollNativeLiveVoice(): Promise<void> {
  if (nativePollActive || transportMode !== "native") return;
  nativePollActive = true;
  try {
    const events = await liveVoiceDrainNativeEvents();
    if (transportMode !== "native") return;
    let acknowledgedThrough = 0;
    for (const envelope of events) {
      handleServerEvent(JSON.stringify(envelope.event));
      acknowledgedThrough = envelope.sequence;
    }
    if (acknowledgedThrough > 0) await liveVoiceAckNativeEvents(acknowledgedThrough);
    const next = await liveVoiceStatus();
    if (transportMode !== "native") return;
    if (next.phase === "failed" || !next.active) {
      closeMediaTransport();
      updateClientState({ active: false, phase: "failed", error: next.error ?? "Native Live ended" });
      return;
    }
    updateClientState({ muted: next.muted, phase: next.phase });
  } catch (error) {
    if (transportMode === "native") updateClientState({ error: `Native Live sync failed: ${String(error)}` });
  } finally {
    nativePollActive = false;
  }
}

async function connectNativeLiveVoice(workshopName: string, sessionId: string): Promise<void> {
  transportMode = "native";
  const started = await liveVoiceStartNative(workshopName, sessionId);
  if (started.phase === "failed") throw new Error(started.error ?? "Native Live could not start");
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (workAbort.signal.aborted) throw new Error("Live connection was stopped.");
    const status = await liveVoiceStatus();
    if (status.phase === "failed") throw new Error(status.error ?? "Native Live could not connect");
    if (status.active && ["listening", "muted"].includes(status.phase)) {
      updateClientState({
        active: true,
        muted: status.muted,
        phase: status.phase,
        liveSessionId: status.liveSessionId ?? "native-live",
      });
      sessionReady = true;
      nativeStatusTimer = window.setInterval(() => {
        void pollNativeLiveVoice();
      }, 500);
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 100));
  }
  throw new Error("Native Live did not confirm startup");
}

async function handleLiveDelegation(event: Record<string, unknown>) {
  const delegation = liveDelegation(event);
  if (!delegation || handledToolCalls.has(delegation.id)) return;
  handledToolCalls.add(delegation.id);
  const generation = transportGeneration;
  const handler = handoffHandler;
  const signal = workAbort.signal;
  // Transcript delivery may lag delegation metadata. This is a bounded context
  // collection window, NOT a speech/turn-completed detector.
  await new Promise((resolve) => window.setTimeout(resolve, 750));
  if (signal.aborted) return;
  const transcript = timeline.snapshot(delegation.offsetMs);
  const request = timeline.requestBetween(lastDelegationOffset, delegation.offsetMs);
  const requestStart = timeline.requestStart(lastDelegationOffset, delegation.offsetMs);
  lastDelegationOffset = Math.max(lastDelegationOffset, delegation.offsetMs);
  let result: LiveWorkResult = { status: "failed", text: "The request's speech context was unavailable. Please repeat it." };
  if (handler) {
    updateClientState({ workStatus: "Working in this chat…" });
    sendRealtimeEvent({ type: "session.thinking.append", event_id: `progress-${delegation.id}`,
      delegation_id: delegation.id, content: "The workshop is processing this request. No result is available yet; tool availability and completion are not known." });
    try {
      const coordinated = await coordinator.execute(request, () => request
        ? handler(request, transcript, signal, (turnId) => {
          if (!canSendLiveEvent(generation)) return;
          if (!transcriptBindings.some((binding) => binding.turnId === turnId)) transcriptBindings.push({ start: requestStart, turnId });
        })
        : Promise.resolve({ status: "failed", text: "The request's speech context was unavailable. Please repeat it." } as LiveWorkResult), signal);
      if (coordinated.kind === "acknowledgment") {
        if (canSendLiveEvent(generation)) sendRealtimeEvent({
          type: "session.thinking.append", event_id: `ack-${delegation.id}`, delegation_id: delegation.id,
          content: "This was a conversational acknowledgment. It did not start another task, cancel pending work, approve any action, or suppress verified results from earlier work."
        });
        return;
      }
      result = coordinated.result;
    }
    catch (error) { result = { status: "failed", text: String(error) }; }
  }
  if (!canSendLiveEvent(generation)) return;
  if (request && result.turnId) delegatedRequests.add(request.trim());
  if (result.turnId && !transcriptBindings.some((binding) => binding.turnId === result.turnId)) transcriptBindings.push({ start: requestStart, turnId: result.turnId });
  latestResult = { id: delegation.id, result };
  updateClientState({ resultAvailable: true });
  presentLiveResult(delegation.id, result, generation, signal);
}

function presentLiveResult(id: string, result: LiveWorkResult, generation: number, signal: AbortSignal) {
  if (signal.aborted || !canSendLiveEvent(generation)) return;
  for (const timer of resultAcks.values()) window.clearTimeout(timer);
  resultAcks.clear();
  const resultEvent = { ...liveDelegationResult(id, result), event_id: `result-${++resultPresentation}-${id}` };
  updateClientState({ workStatus: "Returning result to Live…" });
  resultAcks.set(resultEvent.event_id, window.setTimeout(() => {
    resultAcks.delete(resultEvent.event_id);
    if (signal.aborted || !canSendLiveEvent(generation)) return;
    updateClientState({ workStatus: null, error: "Work finished in chat, but Live did not acknowledge the result." });
  }, 15_000));
  sendRealtimeEvent(resultEvent);
}

/** Presentation only: retrying speech must never rerun a tool task. */
export function hearLatestLiveResult(): void {
  if (!latestResult || !sessionReady) return;
  presentLiveResult(latestResult.id, latestResult.result, transportGeneration, workAbort.signal);
}

function waitForIceGathering(connection: RTCPeerConnection): Promise<void> {
  if (connection.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => finish(new Error("The iPhone could not gather a Live audio route")), 10_000);
    const finish = (error?: Error) => {
      window.clearTimeout(timeout);
      connection.removeEventListener("icegatheringstatechange", onChange);
      if (error) reject(error);
      else resolve();
    };
    const onChange = () => {
      if (connection.iceGatheringState !== "complete") return;
      finish();
    };
    connection.addEventListener("icegatheringstatechange", onChange);
  });
}

function waitForConnection(connection: RTCPeerConnection): Promise<void> {
  if (connection.connectionState === "connected") return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => finish(new Error("Medousa Live timed out")), 20_000);
    const finish = (error?: Error) => {
      window.clearTimeout(timeout);
      connection.removeEventListener("connectionstatechange", onChange);
      if (error) reject(error);
      else resolve();
    };
    const onChange = () => {
      if (connection.connectionState === "connected") finish();
      else if (["failed", "closed"].includes(connection.connectionState)) {
        finish(new Error("Medousa Live could not connect"));
      }
    };
    connection.addEventListener("connectionstatechange", onChange);
  });
}

export function livePhaseForServerEvent(type: string): LiveVoicePhase | null {
  if (type.includes("output_audio") && type.endsWith("started")) return "speaking";
  if (type.includes("input_audio") && type.endsWith("speech_started")) return "listening";
  if (type.includes("response") && type.endsWith("started")) return "thinking";
  if (type.includes("response") && type.endsWith("done")) return "listening";
  return null;
}

export function liveTranscriptForServerEvent(event: Record<string, unknown>): LiveTranscriptEntry | null {
  const type = typeof event.type === "string" ? event.type : "";
  const text = typeof event.transcript === "string" ? event.transcript.trim() : "";
  if (!text) return null;
  const id =
    (typeof event.item_id === "string" && event.item_id) ||
    (typeof event.response_id === "string" && event.response_id) ||
    (typeof event.event_id === "string" && event.event_id) ||
    `${type}-${text}`;
  if (type === "conversation.item.input_audio_transcription.completed") {
    return { id: `user-${id}`, role: "user", text };
  }
  if (type === "response.output_audio_transcript.done") {
    return { id: `assistant-${id}`, role: "assistant", text };
  }
  return null;
}

function appendTranscript(entry: LiveTranscriptEntry) {
  liveVoiceState.update((current) => {
    const existing = current.transcript.findIndex((item) => item.id === entry.id);
    const transcript = [...current.transcript];
    if (existing >= 0) transcript[existing] = entry;
    else transcript.push(entry);
    return { ...current, transcript: transcript.slice(-20) };
  });
}

function sendRealtimeEvent(event: Record<string, unknown>) {
  if (transportMode === "native") {
    void liveVoiceSendNativeEvent(event).catch((error) => {
      updateClientState({ error: `Native Live rejected an update: ${String(error)}` });
    });
    return;
  }
  if (dataChannel?.readyState === "open") dataChannel.send(JSON.stringify(event));
}

function flushWorkNarration() {
  if (responseActive || dataChannel?.readyState !== "open") return;
  const event = pendingNarrations.shift();
  if (!event) return;
  responseActive = true;
  sendRealtimeEvent(event);
}

async function handleHandoffTool(event: Record<string, unknown>) {
  if (event.type !== "response.function_call_arguments.done") return;
  if (event.name !== "hand_off_to_medousa" || typeof event.call_id !== "string") return;
  if (handledToolCalls.has(event.call_id)) return;
  handledToolCalls.add(event.call_id);
  const originChannel = dataChannel;
  const originHandler = handoffHandler;
  const signal = workAbort.signal;

  let request = "";
  try {
    if (typeof event.arguments === "string") {
      const args = JSON.parse(event.arguments) as { request?: unknown };
      if (typeof args.request === "string") request = args.request.trim();
    }
  } catch {
    // Report malformed arguments to the voice model through the normal tool result.
  }

  let result: LiveWorkResult = { status: "failed", text: "The request could not be started because it was empty or Live has no work handler." };
  if (request && originHandler) {
    updateClientState({ phase: "thinking" });
    try {
      await transcriptWrites;
      const transcript = [...get(liveVoiceState).transcript];
      if (signal.aborted) return;
      result = await originHandler(request, transcript, signal);
    } catch (error) {
      result = { status: "failed", text: `The Medousa request failed: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  if (signal.aborted || dataChannel !== originChannel || originChannel?.readyState !== "open") return;
  const [outputEvent, narrationEvent] = liveWorkResultEvents(event.call_id, result);
  sendRealtimeEvent(outputEvent);
  // A user may be talking when work finishes. Never start overlapping responses.
  pendingNarrations.push(narrationEvent);
  flushWorkNarration();
}

function handleServerEvent(raw: string) {
  try {
    const event = JSON.parse(raw) as Record<string, unknown>;
    if (typeof event.type !== "string") return;
    if (protocol === "live") {
      if (event.type === "session.commentary.appended" && typeof event.client_event_id === "string") {
        const timer = resultAcks.get(event.client_event_id);
        if (timer) {
          window.clearTimeout(timer);
          resultAcks.delete(event.client_event_id);
          updateClientState({ workStatus: "Result accepted by Live", error: null });
        }
      }
      if (event.type === "session.started") sessionReady = true;
      if (event.type === "session.closed") {
        const usage = event.usage as { seconds?: unknown } | undefined;
        liveVoiceUsage.set({ seconds: typeof usage?.seconds === "number" ? usage.seconds : null, confirmed: true });
        sessionReady = false;
        sessionFinalized?.();
        if (!workAbort.signal.aborted) {
          const reason = typeof event.reason === "string" ? event.reason : "unknown";
          void disconnectLiveVoice().then(() => {
            updateClientState({ phase: "failed", error: `Live session ended (${reason}). Tool work remains in chat.` });
          });
        }
      }
      if (event.type === "session.usage.updated") {
        const usage = event.usage as { seconds?: unknown } | undefined;
        liveVoiceUsage.set({ seconds: typeof usage?.seconds === "number" ? usage.seconds : null, confirmed: false });
      }
      if (event.type === "error") {
        const error = event.error as { message?: string; client_event_id?: string } | undefined;
        if (error?.client_event_id) {
          const timer = resultAcks.get(error.client_event_id);
          if (timer) { window.clearTimeout(timer); resultAcks.delete(error.client_event_id); }
        }
        updateClientState({ workStatus: null, error: error?.message ?? "Live rejected an update" });
      }
      if (timeline.accept(event)) {
        updateClientState({ transcript: timeline.snapshot().slice(-20) });
        if (event.type === "session.output_transcript.delta" && resultAcks.size === 0
          && get(liveVoiceState).workStatus === "Result accepted by Live") {
          updateClientState({ workStatus: null });
        }
      }
      void handleLiveDelegation(event);
      return;
    }
    if (event.type === "response.created") responseActive = true;
    if (event.type === "response.done") {
      responseActive = false;
      flushWorkNarration();
    }
    const phase = livePhaseForServerEvent(event.type);
    if (phase) updateClientState({ phase });
    const transcript = liveTranscriptForServerEvent(event);
    if (transcript) {
      appendTranscript(transcript);
      persistTranscript(transcript);
    }
    void handleHandoffTool(event);
  } catch {
    // Ignore non-JSON diagnostic frames; media continues independently.
  }
}

export async function connectLiveVoice(
  workshopName: string,
  sessionId: string,
  onHandoff?: LiveHandoffHandler,
  onTranscriptSaved?: () => Promise<void>,
  requestedProtocol: "live" | "realtime" = "live",
): Promise<void> {
  if (!isTauriIos()) throw new Error(unavailable.error ?? "Medousa Live is unavailable");

  await disconnectLiveVoice();
  workAbort = new AbortController();
  protocol = requestedProtocol;
  timeline = new LiveTimeline();
  lastDelegationOffset = -1;
  delegatedRequests.clear();
  transcriptBindings = [];
  coordinator = new LiveDelegationCoordinator();
  latestResult = null;
  liveVoiceUsage.set({ seconds: null, confirmed: false });
  const connectionSignal = workAbort.signal;
  const requireCurrentConnection = () => {
    if (connectionSignal.aborted) throw new Error("Live connection was stopped.");
  };
  handoffHandler = onHandoff ?? null;
  transcriptSavedHandler = onTranscriptSaved ?? null;
  updateClientState({
    available: true,
    active: false,
    muted: false,
    phase: "connecting",
    workshopName,
    sessionId,
    liveSessionId: null,
    transcript: [],
    workStatus: null,
    resultAvailable: false,
    error: null,
  });

  try {
    if (nativeLivePreviewEnabled() && requestedProtocol === "live") {
      try {
        await connectNativeLiveVoice(workshopName, sessionId);
        return;
      } catch (error) {
        console.warn("[live] native preview startup failed; falling back to WebRTC", error);
        await liveVoiceStop().catch(() => undefined);
        transportMode = "webrtc";
      }
    }
    if (!navigator.mediaDevices?.getUserMedia || typeof RTCPeerConnection === "undefined") {
      throw new Error("This iPhone does not expose the required WebRTC audio APIs");
    }
    const nativeStatus = await liveVoiceStart(workshopName, sessionId);
    requireCurrentConnection();
    if (nativeStatus.phase === "failed") {
      throw new Error(nativeStatus.error ?? "iOS could not start the Live audio session");
    }
    localStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    requireCurrentConnection();

    const connection = new RTCPeerConnection();
    peer = connection;
    connection.addEventListener("connectionstatechange", () => {
      if (peer !== connection || connection.connectionState !== "failed") return;
      closeMediaTransport();
      void liveVoiceStop().catch(() => undefined);
      updateClientState({
        active: false,
        phase: "failed",
        error: "The Live audio connection was lost",
      });
    });
    for (const track of localStream.getAudioTracks()) connection.addTrack(track, localStream);

    remoteAudio = document.createElement("audio");
    remoteAudio.autoplay = true;
    remoteAudio.setAttribute("playsinline", "true");
    remoteAudio.hidden = true;
    document.body.append(remoteAudio);
    connection.addEventListener("track", (event) => {
      if (!remoteAudio) return;
      remoteAudio.srcObject = event.streams[0] ?? new MediaStream([event.track]);
      void remoteAudio.play().catch(() => undefined);
    });

    dataChannel = connection.createDataChannel("oai-events");
    let seedHistory: NonNullable<LiveSessionAnswer["seedHistory"]> = [];
    dataChannel.addEventListener("open", () => {
      if (peer !== connection) return;
      if (protocol === "live") return;
      for (const event of liveHistoryEvents(seedHistory)) sendRealtimeEvent(event);
    }, { once: true });
    dataChannel.addEventListener("message", (event) => {
      if (peer !== connection) return;
      if (typeof event.data === "string") handleServerEvent(event.data);
    });

    await connection.setLocalDescription(await connection.createOffer());
    await waitForIceGathering(connection);
    const offer = connection.localDescription?.sdp;
    if (!offer) throw new Error("The iPhone could not create a Live audio offer");

    const answer = await createLiveSession(offer, sessionId, workshopName, requestedProtocol);
    requireCurrentConnection();
    seedHistory = answer.seedHistory ?? [];
    updateClientState({ liveSessionId: answer.liveSessionId });
    await connection.setRemoteDescription({ type: "answer", sdp: answer.sdp });
    await waitForConnection(connection);
    if (protocol === "live") {
      const deadline = Date.now() + 20_000;
      while (!sessionReady) {
        requireCurrentConnection();
        if (Date.now() > deadline) throw new Error("OpenAI Live did not confirm session startup");
        await new Promise((resolve) => window.setTimeout(resolve, 50));
      }
    }
    requireCurrentConnection();
    updateClientState({
      active: true,
      phase: "listening",
      liveSessionId: answer.liveSessionId,
    });
  } catch (error) {
    await disconnectLiveVoice();
    updateClientState({
      active: false,
      phase: "failed",
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}

export async function setLiveVoiceMuted(muted: boolean): Promise<void> {
  for (const track of localStream?.getAudioTracks() ?? []) track.enabled = !muted;
  await liveVoiceSetMuted(muted);
  updateClientState({ muted, phase: muted ? "muted" : "listening" });
}

export async function disconnectLiveVoice(): Promise<void> {
  workAbort.abort();
  if (protocol === "live") {
    // Persist only stable snapshots at closure, not every growing delta. Display
    // groups are heuristic; fragments never trigger durable work themselves.
    if (transportMode !== "native" && sessionReady && dataChannel?.readyState === "open") {
      await new Promise<void>((resolve) => {
        const timeout = window.setTimeout(() => {
          sessionFinalized = null;
          updateClientState({ error: "Live closed without confirmed final usage" });
          resolve();
        }, 5_000);
        sessionFinalized = () => { window.clearTimeout(timeout); sessionFinalized = null; resolve(); };
        sendRealtimeEvent({ type: "session.close" });
      });
    }
    for (const [index, slice] of liveTranscriptSlices(timeline, transcriptBindings).entries()) {
      persistTranscript({ id: `attachment-${index}`, role: "assistant", text: JSON.stringify(slice.rows) }, true, slice.turnId);
    }
    await transcriptWrites;
  }
  closeMediaTransport();
  if (isTauriIos()) await liveVoiceStop().catch(() => undefined);
  const finalizationError = transportMode !== "native" && protocol === "live" && get(liveVoiceState).liveSessionId && !get(liveVoiceUsage).confirmed
    ? "Live ended without confirmed final usage" : null;
  transportMode = "webrtc";
  liveVoiceState.set({ ...idle, available: isTauriIos(), error: finalizationError ?? get(liveVoiceState).error });
}

export async function liveVoiceStart(
  workshopName: string,
  sessionId: string,
): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_start", { workshopName, sessionId });
}

export async function liveVoiceStartNative(
  workshopName: string,
  sessionId: string,
): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_start_native", { workshopName, sessionId });
}

export async function liveVoiceSetMuted(muted: boolean): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_set_muted", { muted });
}

export async function liveVoiceStop(): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_stop");
}

export async function liveVoiceStatus(): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_status");
}

export async function liveVoiceDrainNativeEvents(): Promise<NativeLiveEventEnvelope[]> {
  if (!isTauriIos()) return [];
  return invoke<NativeLiveEventEnvelope[]>("live_voice_drain_native_events");
}

export async function liveVoiceAckNativeEvents(throughSequence: number): Promise<void> {
  if (!isTauriIos()) return;
  await invoke("live_voice_ack_native_events", { throughSequence });
}

export async function liveVoiceSendNativeEvent(event: Record<string, unknown>): Promise<void> {
  if (!isTauriIos()) return;
  await invoke("live_voice_send_native_event", { event });
}
