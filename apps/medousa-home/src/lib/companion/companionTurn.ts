import {
  createTurnTicket,
  getActiveSessionTurn,
} from "$lib/daemon";
import { buildInteractiveTurnOptions } from "$lib/interactiveTurnOptions";
import { chat } from "$lib/stores/chat.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import type { TurnTicketResponse } from "$lib/types/session";

export async function sendCompanionPrompt(
  rawPrompt: string,
): Promise<TurnTicketResponse> {
  const prompt = rawPrompt.trim();
  if (!prompt) throw new Error("Write something for Medousa first.");

  let sessionId = chat.sessionId.trim();
  if (!sessionId) {
    await chat.newSession();
    sessionId = chat.sessionId.trim();
  }
  if (!sessionId) throw new Error("Could not create a conversation.");

  let anotherTurnActive = chat.hasLiveInteractiveTurn();
  try {
    const active = await getActiveSessionTurn(sessionId);
    anotherTurnActive ||= active.active;
  } catch {
    // The create call below remains the authoritative availability check.
  }

  const options = buildInteractiveTurnOptions();
  const voice = voicePresets.turnVoiceFields();
  const ticket = await createTurnTicket({
    sessionId,
    prompt,
    mode: anotherTurnActive ? "background" : "interactive",
    provider: options.provider,
    model: options.model,
    responseDepthMode: options.responseDepthMode,
    reasoningEffort: options.reasoningEffort,
    stageRouting: options.stageRouting,
    channelSurface: options.channelSurface,
    voicePresetId: voice.voicePresetId,
    voiceAppendix: voice.voiceAppendix,
    identityUserId: options.identityUserId,
  });

  chat.beginTurn(
    prompt,
    ticket,
    [],
    options.identityUserId ?? userProfiles.activeProfileId,
  );
  await chat.startTurnStream(ticket.turn_id, ticket.session_id, ticket.stream_url);
  return ticket;
}
