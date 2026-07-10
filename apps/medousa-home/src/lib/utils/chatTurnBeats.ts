/** Truncate a user prompt to a one-line whisper hook (Thinking-adjacent). */
export function userWhisperHook(text: string, maxLen = 44): string {
  const first =
    text
      .split("\n")
      .map((line) => line.trim())
      .find(Boolean) ?? "";
  if (!first) return "";
  if (first.length <= maxLen) return first;
  return `${first.slice(0, maxLen - 1)}…`;
}

export type ChatTurnBeat =
  | { kind: "pair"; user: import("$lib/types/chat").ChatMessage; assistant: import("$lib/types/chat").ChatMessage }
  | { kind: "single"; message: import("$lib/types/chat").ChatMessage };

/** Group consecutive user→assistant into presentation beats (no store change). */
export function groupChatTurnBeats(
  messages: import("$lib/types/chat").ChatMessage[],
): ChatTurnBeat[] {
  const beats: ChatTurnBeat[] = [];
  let i = 0;
  while (i < messages.length) {
    const current = messages[i];
    const next = messages[i + 1];
    if (current.role === "user" && next?.role === "assistant") {
      beats.push({ kind: "pair", user: current, assistant: next });
      i += 2;
      continue;
    }
    beats.push({ kind: "single", message: current });
    i += 1;
  }
  return beats;
}

/** Latest user prompt should stay open while its reply is still streaming (or pending). */
export function shouldForceExpandUserWhisper(
  messages: import("$lib/types/chat").ChatMessage[],
  userId: string,
): boolean {
  let lastUserIndex = -1;
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === "user") {
      lastUserIndex = i;
      break;
    }
  }
  if (lastUserIndex < 0 || messages[lastUserIndex].id !== userId) return false;
  const following = messages[lastUserIndex + 1];
  if (!following) return true;
  if (following.role === "assistant" && following.streaming) return true;
  return false;
}
