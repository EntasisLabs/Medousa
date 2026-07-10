/**
 * Promote a settled assistant chat turn into a Library inbox note.
 * Thin living-stone bridge: markdown body only (same substance as painted prose).
 */

import { createVaultNote } from "$lib/daemon";
import { vault } from "$lib/stores/vault.svelte";
import type { ChatMessage } from "$lib/types/chat";
import { stripChatBodyChrome } from "$lib/liquid/surfaces/chat/stripChatBodyChrome";
import { userWhisperHook } from "$lib/utils/chatTurnBeats";
import {
  openSavedVaultNote,
  showSaveFeedback,
  type SaveToLibraryResult,
} from "$lib/utils/saveBrowserPage";
import { inboxCapturePath, slugifyTitle } from "$lib/utils/vaultTemplates";

const TITLE_MAX = 72;
const PREAMBLE_MAX = 280;

export function chatTurnTitle(markdown: string): string {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  let inFence = false;
  let firstLine: string | null = null;

  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith("```")) {
      inFence = !inFence;
      continue;
    }
    if (inFence || line === "---") continue;
    const heading = line.match(/^#{1,6}\s+(.+)$/);
    if (heading?.[1]) {
      return truncateTitle(heading[1].trim());
    }
    if (firstLine == null) {
      firstLine = line.replace(/^[*_]+|[*_]+$/g, "").trim();
    }
  }

  if (firstLine) return truncateTitle(firstLine);
  return "Chat turn";
}

function truncateTitle(text: string): string {
  if (text.length <= TITLE_MAX) return text || "Chat turn";
  return `${text.slice(0, TITLE_MAX - 1)}…`;
}

export function assembleChatTurnNoteBody(options: {
  assistantMarkdown: string;
  userPrompt?: string | null;
  title: string;
}): string {
  const parts: string[] = [`# ${options.title}`, ""];
  const prompt = options.userPrompt?.trim();
  if (prompt) {
    const hook =
      prompt.length > PREAMBLE_MAX ? `${prompt.slice(0, PREAMBLE_MAX - 1)}…` : prompt;
    const quoted = hook
      .split("\n")
      .map((line) => `> ${line}`)
      .join("\n");
    parts.push(`> **You**`, quoted, "");
  }
  parts.push(options.assistantMarkdown.trim(), "");
  return parts.join("\n");
}

export function assembleChatTurnNoteContent(options: {
  assistantMarkdown: string;
  userPrompt?: string | null;
  title: string;
  turnIndex?: number | null;
}): string {
  const body = assembleChatTurnNoteBody(options);
  const meta: string[] = ["kind: inbox", "tags: [chat-turn]"];
  if (options.turnIndex != null && Number.isFinite(options.turnIndex)) {
    meta.push(`turn_index: ${Math.trunc(options.turnIndex)}`);
  }
  return `---\n${meta.join("\n")}\n---\n\n${body}`;
}

/** Whether the quiet Save action should appear on this assistant message. */
export function canSaveAssistantTurn(message: ChatMessage): boolean {
  if (message.role !== "assistant") return false;
  if (message.streaming) return false;
  if (message.failed && !message.content?.trim()) return false;
  const body = stripChatBodyChrome(message.content ?? "").markdown.trim();
  return body.length > 0;
}

export async function saveChatTurnToVault(options: {
  assistant: ChatMessage;
  user?: ChatMessage | null;
  sessionId: string;
  /** When true, open the note after save (default false — toast offers Open). */
  openNote?: boolean;
}): Promise<SaveToLibraryResult> {
  if (!canSaveAssistantTurn(options.assistant)) {
    return { status: "error", message: "Nothing to save yet" };
  }

  const stripped = stripChatBodyChrome(options.assistant.content ?? "").markdown;
  const title = chatTurnTitle(stripped);
  const userPrompt = options.user?.content?.trim() || null;
  const content = assembleChatTurnNoteContent({
    assistantMarkdown: stripped,
    userPrompt,
    title,
    turnIndex: options.assistant.turnIndex ?? null,
  });

  // Prefer a readable slug path; fall back to stamped capture if create fails later.
  const slug = slugifyTitle(title);
  const path = `inbox/${slug}.md`;
  const stamped = inboxCapturePath();

  try {
    let response;
    try {
      response = await createVaultNote(path, content, {
        sessionId: options.sessionId || undefined,
        semanticTags: ["chat-turn"],
      });
    } catch {
      // Collision or path conflict — use unique stamp.
      response = await createVaultNote(stamped, content, {
        sessionId: options.sessionId || undefined,
        semanticTags: ["chat-turn"],
      });
    }

    const savedPath = response.note.path;
    await vault.refreshNotes();
    if (options.openNote) {
      openSavedVaultNote(savedPath);
    }
    return { status: "saved", path: savedPath };
  } catch (err) {
    return {
      status: "error",
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

export function showChatTurnSaveFeedback(result: SaveToLibraryResult) {
  showSaveFeedback(result);
}

/** Compact label for UI (optional; mirrors whisper hook). */
export function chatTurnSaveHint(userPrompt: string | null | undefined): string {
  if (!userPrompt?.trim()) return "Save to Library";
  return `Save · ${userWhisperHook(userPrompt, 28)}`;
}
