/**
 * Turn command helpers — user/assistant bubble pair and ticket registration.
 * Stream attach/cancel still go through the chat façade until H10 transport.
 */

import type { ChatMessage, TurnTicketState } from "$lib/types/chat";
import type { TurnTicketResponse } from "$lib/types/session";
import type { MediaRef } from "$lib/types/media";
import { chatMediaAttachmentsFromRefs } from "$lib/utils/chatMediaUpload";

export function turnStateFromTicket(
  ticket: TurnTicketResponse,
  messageId: string | null,
): TurnTicketState {
  return {
    turnId: ticket.turn_id,
    mode: ticket.mode,
    phase: ticket.phase,
    messageId,
    streamAttached: true,
    terminal: false,
    workspaceCardId: ticket.workspace_card_id ?? null,
  };
}

export function beginTurnMessages(input: {
  userContent: string;
  ticket: TurnTicketResponse;
  mediaRefs?: MediaRef[];
  speakerProfileId?: string | null;
  userMessageId: string;
  assistantId: string;
}): ChatMessage[] {
  const isAsk = input.ticket.mode === "background";
  const askJobId = input.ticket.workspace_card_id ?? input.ticket.turn_id;
  const lane = isAsk ? ("ask" as const) : ("chat" as const);
  const speaker =
    typeof input.speakerProfileId === "string" && input.speakerProfileId.trim()
      ? input.speakerProfileId.trim()
      : null;
  const mediaRefs = input.mediaRefs ?? [];
  return [
    {
      id: input.userMessageId,
      role: "user",
      content: input.userContent,
      turnId: input.ticket.turn_id,
      lane,
      askJobId: isAsk ? askJobId : null,
      speakerProfileId: speaker,
      mediaAttachments:
        mediaRefs.length > 0 ? chatMediaAttachmentsFromRefs(mediaRefs) : undefined,
    },
    {
      id: input.assistantId,
      role: "assistant",
      content: "",
      streaming: true,
      turnId: input.ticket.turn_id,
      lane,
      askJobId: isAsk ? askJobId : null,
      statusLine: input.ticket.mode === "background" ? "Background turn started" : null,
    },
  ];
}
