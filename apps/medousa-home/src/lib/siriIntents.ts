import { invoke } from "@tauri-apps/api/core";
import { createTurnTicket } from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";

type PendingSiriAsk = {
  requestId: string;
  prompt: string;
  createdAt: number;
};

export async function startPendingSiriAsk(requestId: string): Promise<void> {
  const workshopEpoch = chat.workshopEpoch;
  if (!chat.workshopScopeId) {
    throw new Error("Workshop is switching; wait for it to reconnect");
  }
  const pending = await invoke<PendingSiriAsk>("siri_consume_pending_ask", {
    requestId,
  });

  const accepted = await createTurnTicket({
    sessionId: chat.sessionId,
    prompt: pending.prompt,
    mode: "interactive",
    channelSurface: "home-ios-siri",
  });
  if (chat.workshopEpoch !== workshopEpoch) {
    throw new Error("Workshop changed while the Siri request was being admitted");
  }
  chat.beginTurn(pending.prompt, accepted);
  await chat.startTurnStream(accepted.turn_id, accepted.session_id, accepted.stream_url);
}
