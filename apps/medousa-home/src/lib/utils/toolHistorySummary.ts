import type { ToolHistoryRunEntry } from "$lib/types/toolHistory";
import { formatToolName } from "$lib/utils/formatTurn";

/** Primary line for a history beat — prose, never raw JSON. */
export function humanToolRunHeadline(entry: ToolHistoryRunEntry): string {
  const tool = formatToolName(entry.tool_name);
  const outcome = humanizeBlob(entry.output_preview?.trim() ?? "");
  const input = humanizeBlob(entry.input_summary.trim());

  if (outcome && !isBoringOutcome(outcome)) {
    if (isShortProse(outcome)) return outcome;
    return `${verbForTool(entry.tool_name, tool)} — ${truncate(outcome, 88)}`;
  }

  if (input) {
    const fromJson = summarizeStructured(entry.tool_name, entry.input_summary);
    if (fromJson) return fromJson;
    if (isShortProse(input)) return input;
    return `${verbForTool(entry.tool_name, tool)} — ${truncate(input, 88)}`;
  }

  return tool;
}

/** Quiet secondary line under the beat. */
export function humanToolRunSubline(entry: ToolHistoryRunEntry): string {
  return formatToolName(entry.tool_name);
}

export function humanToolRunDetail(entry: ToolHistoryRunEntry): string {
  const slice = entry.slice_id.trim();
  if (slice.length <= 28) return slice;
  return `${slice.slice(0, 12)}…${slice.slice(-10)}`;
}

/** What she took in — prose for the expanded beat. */
export function humanToolRunAsk(entry: ToolHistoryRunEntry): string | null {
  const raw = entry.input_summary.trim();
  if (!raw || raw === "{}" || raw === "[]") return null;
  const structured = summarizeStructured(entry.tool_name, raw, { preferDetail: true });
  if (structured) return structured;
  const cleaned = humanizeBlob(raw);
  return cleaned ? truncate(cleaned, 220) : null;
}

/** What came back — prose for the expanded beat. */
export function humanToolRunResult(entry: ToolHistoryRunEntry): string | null {
  const raw = entry.output_preview?.trim() ?? "";
  if (!raw) return null;
  const cleaned = humanizeBlob(raw);
  if (!cleaned || isBoringOutcome(cleaned)) return null;
  return truncate(cleaned, 220);
}

export function sessionChapterTitle(entry: ToolHistoryRunEntry): string {
  const preview = entry.session_preview?.trim() ?? "";
  if (preview) {
    return preview.replace(/^#\s*/, "").trim() || "Untitled chat";
  }
  return "Untitled chat";
}

export function suggestFlowNameFromRun(entry: ToolHistoryRunEntry): string {
  const chapter = sessionChapterTitle(entry);
  if (chapter !== "Untitled chat") {
    return `Repeat: ${truncate(chapter, 48)}`;
  }
  const headline = humanToolRunHeadline(entry);
  if (headline && !looksLikeJson(headline)) {
    return `Repeat: ${truncate(headline, 48)}`;
  }
  return `Repeat ${formatToolName(entry.tool_name)}`;
}

export function suggestFlowNameFromRuns(entries: ToolHistoryRunEntry[]): string {
  if (entries.length === 0) return "";
  const chapters = new Set(entries.map(sessionChapterTitle));
  if (chapters.size === 1) {
    const only = [...chapters][0];
    if (only !== "Untitled chat") return `Repeat: ${truncate(only, 48)}`;
  }
  return suggestFlowNameFromRun(entries[0]);
}

function verbForTool(toolName: string, fallback: string): string {
  const key = toolName.toLowerCase();
  if (key.includes("remember") || key.includes("identity")) return "Remembered";
  if (key.includes("search") || key.includes("web")) return "Searched";
  if (key.includes("read") || key.includes("fetch")) return "Read";
  if (key.includes("write") || key.includes("edit")) return "Wrote";
  if (key.includes("begin") || key.includes("turn")) return "Began";
  if (key.includes("schedule") || key.includes("cron")) return "Scheduled";
  return fallback;
}

function summarizeStructured(
  toolName: string,
  raw: string,
  options?: { preferDetail?: boolean },
): string | null {
  if (!looksLikeJson(raw)) return null;
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const factKind = readString(parsed.fact_kind);
    const fact = readString(parsed.fact) ?? readString(parsed.value) ?? readString(parsed.text);
    if (factKind === "preference" && fact) {
      return options?.preferDetail ? `Preference: ${truncate(fact, 120)}` : `Remembered a preference`;
    }
    if (factKind && fact) {
      return options?.preferDetail
        ? `${titleCase(factKind)}: ${truncate(fact, 120)}`
        : `Remembered a ${factKind}`;
    }
    if (fact) {
      return options?.preferDetail ? truncate(fact, 160) : `Remembered: ${truncate(fact, 72)}`;
    }

    const query =
      readString(parsed.query) ??
      readString(parsed.q) ??
      readString(parsed.prompt) ??
      readString(parsed.message);
    if (query) {
      return options?.preferDetail
        ? truncate(query, 180)
        : `${verbForTool(toolName, formatToolName(toolName))} “${truncate(query, 64)}”`;
    }

    const prefix = readString(parsed.prefix);
    if (prefix) {
      return `Files matching “${truncate(prefix, 48)}”`;
    }
    const domain = readString(parsed.domain);
    if (domain) {
      return `${domain} tools`;
    }

    // Known keys exhausted — refuse to dump JSON into the UI.
    return verbForTool(toolName, formatToolName(toolName));
  } catch {
    return null;
  }
}

function humanizeBlob(value: string): string {
  if (!value) return "";
  if (!looksLikeJson(value)) return value.replace(/\s+/g, " ").trim();
  try {
    const parsed = JSON.parse(value) as unknown;
    if (typeof parsed === "string") return parsed.trim();
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      const record = parsed as Record<string, unknown>;
      const preferred =
        readString(record.message) ??
        readString(record.text) ??
        readString(record.summary) ??
        readString(record.result) ??
        readString(record.fact) ??
        readString(record.value);
      if (preferred) return preferred;
      const factKind = readString(record.fact_kind);
      if (factKind) return titleCase(factKind);
    }
  } catch {
    /* fall through */
  }
  return "";
}

function isBoringOutcome(value: string): boolean {
  const lower = value.toLowerCase();
  return lower === "ok" || lower === "done" || lower === "success" || lower === "{}";
}

function isShortProse(value: string): boolean {
  return value.length <= 110 && !looksLikeJson(value);
}

function looksLikeJson(value: string): boolean {
  const trimmed = value.trim();
  return (
    (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
    (trimmed.startsWith("[") && trimmed.endsWith("]"))
  );
}

function titleCase(value: string): string {
  return value
    .replaceAll("_", " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function readString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1)}…`;
}
