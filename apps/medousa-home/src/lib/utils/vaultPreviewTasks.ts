/** Phase G2 — interactive task checkboxes in vault markdown preview. */

export const TASK_ITEM_RE = /^(\s*)-\s+\[([ xX])\]\s+(.*)$/;
const DONE_STAMP_RE = /\s*\(done\s+\d{4}-\d{2}-\d{2}\)\s*$/i;

export interface PreviewTaskRef {
  lineIndex: number;
}

function bodyStartLineIndex(lines: string[]): number {
  let start = 0;
  while (start < lines.length && lines[start].trim() === "") start += 1;
  if (lines[start]?.trim() !== "---") return start;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i].trim() === "---") return i + 1;
  }
  return start;
}

/** Task lines in note body, as absolute line indices in the full markdown file. */
export function enumeratePreviewTaskRefs(fullMarkdown: string): PreviewTaskRef[] {
  const lines = fullMarkdown.replace(/\r\n/g, "\n").split("\n");
  const bodyStart = bodyStartLineIndex(lines);
  const refs: PreviewTaskRef[] = [];
  let inFence = false;

  for (let i = bodyStart; i < lines.length; i += 1) {
    const trimmed = lines[i].trimStart();
    if (trimmed.startsWith("```")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (TASK_ITEM_RE.test(lines[i])) {
      refs.push({ lineIndex: i });
    }
  }

  return refs;
}

function formatDoneDate(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function stripDoneStamp(text: string): string {
  return text.replace(DONE_STAMP_RE, "").trimEnd();
}

export function formatTaskLineToggle(
  line: string,
  checked: boolean,
  stampCompletion: boolean,
  doneDate = new Date(),
): string {
  const match = line.match(TASK_ITEM_RE);
  if (!match) return line;

  const [, indent, , textPart] = match;
  let text = stripDoneStamp(textPart.trimEnd());

  if (checked) {
    if (stampCompletion) {
      text = `${text} (done ${formatDoneDate(doneDate)})`;
    }
    return `${indent}- [x] ${text}`;
  }

  return `${indent}- [ ] ${text}`;
}

export function togglePreviewTaskInContent(
  fullMarkdown: string,
  taskIndex: number,
  checked: boolean,
  stampCompletion: boolean,
): string | null {
  const refs = enumeratePreviewTaskRefs(fullMarkdown);
  const ref = refs[taskIndex];
  if (!ref) return null;

  const lines = fullMarkdown.replace(/\r\n/g, "\n").split("\n");
  if (ref.lineIndex >= lines.length) return null;

  const nextLine = formatTaskLineToggle(lines[ref.lineIndex], checked, stampCompletion);
  if (nextLine === lines[ref.lineIndex]) return null;

  lines[ref.lineIndex] = nextLine;
  return lines.join("\n");
}
