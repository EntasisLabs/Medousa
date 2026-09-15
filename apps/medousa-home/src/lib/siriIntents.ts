import { invoke } from "@tauri-apps/api/core";
import { createTurnTicket } from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { classifySiriAskFailure } from "$lib/siriIntentErrors";
import { prepareInteractiveTurnOptions } from "$lib/interactiveTurnOptions";

type PendingSiriAsk = {
  requestId: string;
  prompt: string;
  workshopId: string;
  createdAt: number;
};

const WORKSHOP_READY_TIMEOUT_MS = 15_000;
const WORKSHOP_READY_POLL_MS = 100;
const SIRI_RESULT_WAIT_MS = 18_000;
const SIRI_RESULT_POLL_MS = 150;
const SIRI_RESULT_MAX_CHARS = 600;

export function siriSpokenSummary(value: string): string {
  const plain = value
    .replace(/```[\s\S]*?```/g, " Code omitted. ")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*[-*+]\s+/gm, "")
    .replace(/[*_~`>]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (plain.length <= SIRI_RESULT_MAX_CHARS) return plain;
  const prefix = plain.slice(0, SIRI_RESULT_MAX_CHARS - 1);
  const boundary = prefix.lastIndexOf(" ");
  return `${prefix.slice(0, boundary > 400 ? boundary : prefix.length).trimEnd()}…`;
}

async function publishSiriResult(requestId: string, turnId: string): Promise<void> {
  const deadline = Date.now() + SIRI_RESULT_WAIT_MS;
  while (Date.now() < deadline) {
    const turn = chat.turns.get(turnId);
    const message = chat.messages.find(
      (candidate) => candidate.turnId === turnId && candidate.role === "assistant",
    );
    if (turn?.terminal || (message && !message.streaming)) {
      const text = siriSpokenSummary(message?.content ?? "");
      if (text) {
        await invoke("siri_publish_ask_result", { requestId, text }).catch(
          () => undefined,
        );
      }
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, SIRI_RESULT_POLL_MS));
  }
}

async function waitForWorkshopScope(): Promise<void> {
  const deadline = Date.now() + WORKSHOP_READY_TIMEOUT_MS;
  while (!chat.workshopScopeId && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, WORKSHOP_READY_POLL_MS));
  }
}

export async function startPendingSiriAsk(requestId: string): Promise<void> {
  await waitForWorkshopScope();
  if (!chat.workshopScopeId) {
    throw new Error(classifySiriAskFailure("Workshop is switching").message);
  }
  if (chat.hasLiveInteractiveTurn()) {
    throw new Error(classifySiriAskFailure("Medousa is already working").message);
  }
  let pending: PendingSiriAsk;
  try {
    pending = await invoke<PendingSiriAsk>("siri_consume_pending_ask", {
      requestId,
    });
  } catch (error) {
    throw new Error(classifySiriAskFailure(error).message);
  }

  if (!pending.workshopId) {
    chat.prefillDraft(pending.prompt);
    throw new Error("Choose a workshop in Medousa, then ask Siri again.");
  }
  if (pending.workshopId !== workshops.activeWorkshopId) {
    try {
      await workshops.selectWorkshop(pending.workshopId, { force: true });
      if (workshops.activeWorkshopId !== pending.workshopId) {
        throw new Error("The selected workshop could not be activated");
      }
    } catch (error) {
      chat.prefillDraft(pending.prompt);
      throw new Error(classifySiriAskFailure(error).message);
    }
  }
  const workshopEpoch = chat.workshopEpoch;

  let accepted;
  try {
    const options = await prepareInteractiveTurnOptions(chat);
    accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt: pending.prompt,
      mode: "interactive",
      provider: options.provider,
      model: options.model,
      responseDepthMode: options.responseDepthMode,
      reasoningEffort: options.reasoningEffort,
      stageRouting: options.stageRouting,
      channelSurface: "home-ios-siri",
      browserDriverId: options.browserDriverId,
      selectedWorlds: options.selectedWorlds,
      identityUserId: options.identityUserId,
    });
  } catch (error) {
    chat.prefillDraft(pending.prompt);
    throw new Error(classifySiriAskFailure(error).message);
  }
  if (chat.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the Siri request was being admitted");
  }
  chat.beginTurn(pending.prompt, accepted);
  await chat.startTurnStream(accepted.turn_id, accepted.session_id, accepted.stream_url);
  void publishSiriResult(pending.requestId, accepted.turn_id);
}
