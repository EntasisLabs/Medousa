import type { ToolHistoryRunEntry } from "$lib/types/toolHistory";
import { formatToolName } from "$lib/utils/formatTurn";

export function humanToolRunHeadline(entry: ToolHistoryRunEntry): string {
  const tool = formatToolName(entry.tool_name);
  const input = entry.input_summary.trim();

  if (entry.output_preview?.trim()) {
    const preview = entry.output_preview.trim();
    if (preview.length <= 100) {
      return `${tool}: ${preview}`;
    }
    return `${tool} — ${preview.slice(0, 97)}…`;
  }

  if (!input || input === "{}" || input === "[]") {
    return tool;
  }

  try {
    const parsed = JSON.parse(input) as Record<string, unknown>;
    const query =
      readString(parsed.query) ??
      readString(parsed.q) ??
      readString(parsed.prompt) ??
      readString(parsed.message);
    if (query) {
      return `${tool}: “${truncate(query, 72)}”`;
    }
    const prefix = readString(parsed.prefix);
    if (prefix) {
      return `${tool}: files matching “${truncate(prefix, 48)}”`;
    }
    const domain = readString(parsed.domain);
    if (domain) {
      return `${tool}: ${domain} tools`;
    }
  } catch {
    /* plain text summary */
  }

  if (input.length <= 90) {
    return `${tool}: ${input}`;
  }
  return `${tool} — ${input.slice(0, 87)}…`;
}

export function humanToolRunDetail(entry: ToolHistoryRunEntry): string {
  return `${entry.slice_id} · ${entry.session_id}`;
}

export function suggestFlowNameFromRun(entry: ToolHistoryRunEntry): string {
  const tool = formatToolName(entry.tool_name);
  try {
    const parsed = JSON.parse(entry.input_summary) as Record<string, unknown>;
    const query = readString(parsed.query) ?? readString(parsed.q);
    if (query) {
      return `${tool}: ${truncate(query, 40)}`;
    }
  } catch {
    /* ignore */
  }
  return `Repeat ${tool}`;
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
