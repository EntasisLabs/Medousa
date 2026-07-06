/** Dismissible chat hint — tool receipts can lag behind what Medousa says in prose. */

const ASYNC_TOOLS_HINT_DISMISSED_KEY = "medousa-chat-async-tools-hint-dismissed";

export function isChatAsyncToolsHintDismissed(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(ASYNC_TOOLS_HINT_DISMISSED_KEY) === "1";
}

export function dismissChatAsyncToolsHint(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(ASYNC_TOOLS_HINT_DISMISSED_KEY, "1");
}

export function resetChatAsyncToolsHintDismissed(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(ASYNC_TOOLS_HINT_DISMISSED_KEY);
}
