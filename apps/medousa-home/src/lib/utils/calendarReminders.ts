/** Vault-backed calendar reminders: `- [ ] Title @due(YYYY-MM-DD)`. */

import { createVaultNote, getVaultNote, saveVaultNote } from "$lib/daemon";
import type { CalendarReminder } from "$lib/types/calendar";
import {
  formatTaskLineToggle,
  TASK_ITEM_RE,
} from "$lib/utils/vaultPreviewTasks";
import { vaultIfMatchToken } from "$lib/utils/vaultSave";

export const REMINDERS_NOTE_PATH = "calendar/reminders.md";

export const DUE_RE = /@due\((\d{4}-\d{2}-\d{2})\)/i;

const REMINDERS_SEED = `---
title: Reminders
kind: note
tags: [calendar, reminders]
---

# Reminders

Due tasks surface on the Calendar. Use \`@due(YYYY-MM-DD)\` on a checkbox line.
`;

function bodyStartLineIndex(lines: string[]): number {
  let start = 0;
  while (start < lines.length && lines[start].trim() === "") start += 1;
  if (lines[start]?.trim() !== "---") return start;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i].trim() === "---") return i + 1;
  }
  return start;
}

export function parseDueDay(text: string): string | null {
  const match = text.match(DUE_RE);
  return match?.[1] ?? null;
}

export function stripDueMarker(text: string): string {
  return text.replace(DUE_RE, "").replace(/\s+/g, " ").trim();
}

export function parseRemindersFromMarkdown(
  content: string,
  notePath = REMINDERS_NOTE_PATH,
): CalendarReminder[] {
  const lines = content.replace(/\r\n/g, "\n").split("\n");
  const bodyStart = bodyStartLineIndex(lines);
  const out: CalendarReminder[] = [];
  let inFence = false;

  for (let i = bodyStart; i < lines.length; i += 1) {
    const trimmed = lines[i].trimStart();
    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const match = lines[i].match(TASK_ITEM_RE);
    if (!match) continue;
    const checked = match[2].toLowerCase() === "x";
    const textPart = match[3] ?? "";
    const dueDay = parseDueDay(textPart);
    if (!dueDay) continue;
    const title = stripDueMarker(textPart) || "Reminder";
    out.push({
      id: `${notePath}:${i}`,
      title,
      dueDay,
      notePath,
      lineIndex: i,
      completed: checked,
    });
  }
  return out;
}

export async function ensureRemindersNote(): Promise<string> {
  try {
    await getVaultNote(REMINDERS_NOTE_PATH);
    return REMINDERS_NOTE_PATH;
  } catch {
    await createVaultNote(REMINDERS_NOTE_PATH, REMINDERS_SEED);
    return REMINDERS_NOTE_PATH;
  }
}

export async function loadCalendarReminders(): Promise<CalendarReminder[]> {
  try {
    const note = await getVaultNote(REMINDERS_NOTE_PATH);
    return parseRemindersFromMarkdown(note.content ?? "", REMINDERS_NOTE_PATH);
  } catch {
    return [];
  }
}

export async function appendCalendarReminder(
  title: string,
  dueDay: string,
): Promise<CalendarReminder> {
  const path = await ensureRemindersNote();
  const note = await getVaultNote(path);
  const content = (note.content ?? "").replace(/\s*$/, "");
  const cleanTitle = title.trim() || "Reminder";
  const line = `- [ ] ${cleanTitle} @due(${dueDay})`;
  const next = `${content}\n${line}\n`;
  await saveVaultNote(path, next, {
    contentHash: vaultIfMatchToken(note),
  });
  const parsed = parseRemindersFromMarkdown(next, path);
  const created = parsed[parsed.length - 1];
  if (!created) {
    throw new Error("Failed to append reminder");
  }
  return created;
}

export async function toggleCalendarReminder(
  reminder: CalendarReminder,
  completed: boolean,
  stampCompletion = false,
): Promise<void> {
  const note = await getVaultNote(reminder.notePath);
  const lines = (note.content ?? "").replace(/\r\n/g, "\n").split("\n");
  const line = lines[reminder.lineIndex];
  if (!line || !TASK_ITEM_RE.test(line)) {
    throw new Error("Reminder line not found");
  }
  lines[reminder.lineIndex] = formatTaskLineToggle(line, completed, stampCompletion);
  await saveVaultNote(reminder.notePath, `${lines.join("\n")}`, {
    contentHash: vaultIfMatchToken(note),
  });
}

export function meetingNotePath(summary: string, dayKey: string): string {
  const slug = summary
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48) || "meeting";
  return `projects/meetings/${dayKey}-${slug}.md`;
}

export function meetingNoteContent(summary: string, dayKey: string, eventUid?: string): string {
  const title = summary.trim() || "Meeting";
  const uidLine = eventUid ? `\nevent_uid: ${eventUid}` : "";
  return `---
title: ${JSON.stringify(title)}
kind: note
tags: [calendar, meeting]
date: ${dayKey}${uidLine}
---

# ${title}

`;
}
