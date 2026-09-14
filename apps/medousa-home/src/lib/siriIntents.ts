import { invoke } from "@tauri-apps/api/core";
import { createTurnTicket } from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import { classifySiriAskFailure } from "$lib/siriIntentErrors";

type PendingSiriAsk = {
  requestId: string;
  prompt: string;
  createdAt: number;
};

export async function startPendingSiriAsk(requestId: string): Promise<void> {
  const workshopEpoch = chat.workshopEpoch;
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

  let accepted;
  try {
    accepted = await createTurnTicket({
      sessionId: chat.sessionId,
      prompt: pending.prompt,
      mode: "interactive",
      channelSurface: "home-ios-siri",
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
}
