import { invoke } from "@tauri-apps/api/core";
import { isTauriIos } from "$lib/platform";
import { get, writable } from "svelte/store";

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
  error?: string | null;
}

export interface LiveSessionAnswer {
  liveSessionId: string;
  sdp: string;
}

export interface LiveVoiceClientState extends LiveVoiceStatus {
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
) => Promise<void>;

export async function createLiveSession(
  sdp: string,
  sessionId: string,
  workshopName: string,
): Promise<LiveSessionAnswer> {
  return invoke<LiveSessionAnswer>("live_voice_create_session", { sdp, sessionId, workshopName });
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

let peer: RTCPeerConnection | null = null;
let localStream: MediaStream | null = null;
let remoteAudio: HTMLAudioElement | null = null;
let dataChannel: RTCDataChannel | null = null;
let handoffHandler: LiveHandoffHandler | null = null;
const handledToolCalls = new Set<string>();

function updateClientState(next: Partial<LiveVoiceClientState>) {
  liveVoiceState.update((current) => ({ ...current, ...next }));
}

function closeMediaTransport() {
  dataChannel?.close();
  dataChannel = null;
  handoffHandler = null;
  handledToolCalls.clear();
  peer?.close();
  peer = null;
  for (const track of localStream?.getTracks() ?? []) track.stop();
  localStream = null;
  if (remoteAudio) {
    remoteAudio.pause();
    remoteAudio.srcObject = null;
    remoteAudio.remove();
    remoteAudio = null;
  }
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
  if (dataChannel?.readyState === "open") dataChannel.send(JSON.stringify(event));
}

async function handleHandoffTool(event: Record<string, unknown>) {
  if (event.type !== "response.function_call_arguments.done") return;
  if (event.name !== "hand_off_to_medousa" || typeof event.call_id !== "string") return;
  if (handledToolCalls.has(event.call_id)) return;
  handledToolCalls.add(event.call_id);

  let request = "";
  try {
    if (typeof event.arguments === "string") {
      const args = JSON.parse(event.arguments) as { request?: unknown };
      if (typeof args.request === "string") request = args.request.trim();
    }
  } catch {
    // Report malformed arguments to the voice model through the normal tool result.
  }

  let output = "The handoff could not be started because the request was empty.";
  if (request && handoffHandler) {
    updateClientState({ phase: "thinking" });
    try {
      const transcript = [...get(liveVoiceState).transcript];
      await handoffHandler(request, transcript);
      output = "The request was handed to the active Medousa chat successfully. It can continue using tools and will show progress in the app.";
    } catch (error) {
      output = `The Medousa handoff failed: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  sendRealtimeEvent({
    type: "conversation.item.create",
    item: { type: "function_call_output", call_id: event.call_id, output },
  });
  sendRealtimeEvent({
    type: "response.create",
    response: {
      instructions: "Briefly tell the user whether the Medousa handoff succeeded. Do not repeat the full request.",
    },
  });
}

function handleServerEvent(raw: string) {
  try {
    const event = JSON.parse(raw) as Record<string, unknown>;
    if (typeof event.type !== "string") return;
    const phase = livePhaseForServerEvent(event.type);
    if (phase) updateClientState({ phase });
    const transcript = liveTranscriptForServerEvent(event);
    if (transcript) appendTranscript(transcript);
    void handleHandoffTool(event);
  } catch {
    // Ignore non-JSON diagnostic frames; media continues independently.
  }
}

export async function connectLiveVoice(
  workshopName: string,
  sessionId: string,
  onHandoff?: LiveHandoffHandler,
): Promise<void> {
  if (!isTauriIos()) throw new Error(unavailable.error ?? "Medousa Live is unavailable");
  if (!navigator.mediaDevices?.getUserMedia || typeof RTCPeerConnection === "undefined") {
    throw new Error("This iPhone does not expose the required WebRTC audio APIs");
  }

  await disconnectLiveVoice();
  handoffHandler = onHandoff ?? null;
  updateClientState({
    available: true,
    active: false,
    muted: false,
    phase: "connecting",
    workshopName,
    sessionId,
    liveSessionId: null,
    transcript: [],
    error: null,
  });

  try {
    const nativeStatus = await liveVoiceStart(workshopName, sessionId);
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
    dataChannel.addEventListener("message", (event) => {
      if (typeof event.data === "string") handleServerEvent(event.data);
    });

    await connection.setLocalDescription(await connection.createOffer());
    await waitForIceGathering(connection);
    const offer = connection.localDescription?.sdp;
    if (!offer) throw new Error("The iPhone could not create a Live audio offer");

    const answer = await createLiveSession(offer, sessionId, workshopName);
    await connection.setRemoteDescription({ type: "answer", sdp: answer.sdp });
    await waitForConnection(connection);
    updateClientState({
      active: true,
      phase: "listening",
      liveSessionId: answer.liveSessionId,
    });
  } catch (error) {
    closeMediaTransport();
    await liveVoiceStop().catch(() => undefined);
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
  closeMediaTransport();
  if (isTauriIos()) await liveVoiceStop().catch(() => undefined);
  liveVoiceState.set({ ...idle, available: isTauriIos() });
}

export async function liveVoiceStart(
  workshopName: string,
  sessionId: string,
): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_start", { workshopName, sessionId });
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
