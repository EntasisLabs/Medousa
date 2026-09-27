import type { ChatMessage } from "$lib/types/chat";

export interface ChatTurnGeometry {
  id: string;
  top: number;
  bottom: number;
  height: number;
}

export function chatTurnNavigationItems(messages: ChatMessage[]) {
  return messages
    .filter((message) => message.role === "user" && message.content.trim())
    .map((message) => {
      const firstLine = message.content.trim().split("\n")[0];
      return {
        id: message.id,
        text: firstLine.length > 120 ? `${firstLine.slice(0, 119)}…` : firstLine,
        depth: 2,
      };
    });
}

export interface ChatTurnNavigationState {
  activeId: string | null;
  pinnedId: string | null;
}

/** Resolve the user prompt that owns the assistant response currently in view. */
export function resolveChatTurnNavigation(
  turns: ChatTurnGeometry[],
  viewportTop: number,
  viewportHeight: number,
): ChatTurnNavigationState {
  if (turns.length === 0) return { activeId: null, pinnedId: null };

  const activationLine = viewportTop + 64;
  let activeTurn = turns[0];
  for (const turn of turns) {
    if (turn.top <= activationLine) activeTurn = turn;
    else break;
  }

  const responseIsLong = activeTurn.height >= Math.max(280, viewportHeight * 0.8);
  const promptHasLeftTop = activeTurn.top < viewportTop + 8;
  const responseStillVisible = activeTurn.bottom > viewportTop + 96;

  return {
    activeId: activeTurn.id,
    pinnedId:
      responseIsLong && promptHasLeftTop && responseStillVisible
        ? activeTurn.id
        : null,
  };
}
