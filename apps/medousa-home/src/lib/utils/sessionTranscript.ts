import type { SessionHistoryResponse, SessionTurn } from "$lib/types/session";

function headingForRole(role: string): string {
  const normalized = role.trim().toLowerCase();
  if (normalized === "user") return "You";
  if (normalized === "assistant") return "Medousa";
  if (normalized === "system") return "System";
  return role.trim() || "Turn";
}

function turnBody(turn: SessionTurn): string {
  return (turn.content ?? "").trim();
}

/** Build a readable Markdown transcript from session history. */
export function sessionTranscriptMarkdown(
  history: SessionHistoryResponse,
  options?: { title?: string },
): string {
  const title =
    options?.title?.trim() ||
    `Conversation ${history.session_id.slice(0, 8)}`;
  const lines: string[] = [`# ${title}`, ""];

  for (const turn of history.turns ?? []) {
    const body = turnBody(turn);
    if (!body) continue;
    lines.push(`## ${headingForRole(turn.role)}`);
    if (turn.timestamp?.trim()) {
      lines.push(`_${turn.timestamp}_`, "");
    }
    lines.push(body, "");
  }

  if (lines.length <= 2) {
    lines.push("_No messages in this conversation._", "");
  }

  return lines.join("\n").trimEnd() + "\n";
}

export function downloadTextFile(
  filename: string,
  text: string,
  mime = "text/markdown;charset=utf-8",
): void {
  const blob = new Blob([text], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function sessionExportBasename(sessionId: string): string {
  const short = sessionId.trim().slice(0, 8) || "session";
  return `medousa-session-${short}`;
}
