import type { GraphemeRunResponse } from "$lib/types/grapheme";

const OUTPUT_KEYS = [
  "stdout",
  "output_text",
  "final_output_text",
  "response_text",
  "assistant_message",
  "final_text",
  "answer",
  "content",
  "text",
  "transcript_preview",
] as const;

function readText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (value == null || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function readNumber(record: Record<string, unknown>, ...keys: string[]): number | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number" && Number.isFinite(value)) return value;
  }
  return null;
}

function extractOutputText(payload: unknown): string | null {
  if (payload == null) return null;
  if (typeof payload === "string") return readText(payload);

  const record = asRecord(payload);
  if (!record) return null;

  for (const key of OUTPUT_KEYS) {
    const text = readText(record[key]);
    if (text) return text;
  }

  if (Array.isArray(record.transcript)) {
    for (const entry of [...record.transcript].reverse()) {
      const text = extractOutputText(entry);
      if (text) return text;
    }
  }

  for (const key of ["result", "response", "output", "final", "completion", "execution"]) {
    const nested = record[key];
    if (nested && typeof nested === "object") {
      const text = extractOutputText(nested);
      if (text) return text;
    }
  }

  return null;
}

function collectMeta(record: Record<string, unknown>): string | null {
  const parts: string[] = [];
  const duration = readNumber(record, "duration_ms", "elapsed_ms");
  if (duration != null) parts.push(`${duration}ms`);

  const exitCode = readNumber(record, "exit_code");
  if (exitCode != null) parts.push(`exit ${exitCode}`);

  const backend = readText(record.backend);
  if (backend) parts.push(backend);

  if (record.sandboxed === true) parts.push("sandboxed");
  if (record.timed_out === true) parts.push("timed out");

  const execution = asRecord(record.execution);
  if (execution) {
    const outcome = readText(execution.outcome);
    if (outcome && outcome !== "succeeded" && outcome !== "failed") {
      parts.push(outcome);
    }
  }

  return parts.length > 0 ? parts.join(" · ") : null;
}

export interface ParsedGraphemeRunResult {
  succeeded: boolean;
  /** Engineer status line — always "Run succeeded" / "Run failed". */
  headline: string;
  /** Compact facts: duration, exit code, backend. */
  meta: string | null;
  /** Primary console body (stdout or pretty JSON). */
  summary: string | null;
  /** Full diagnostics dump when distinct from summary. */
  details: string | null;
}

export function parseGraphemeRunResult(
  result: GraphemeRunResponse["result"] | null | undefined,
): ParsedGraphemeRunResult | null {
  if (!result) return null;

  const succeeded = result.succeeded ?? false;
  const headline = succeeded ? "Run succeeded" : "Run failed";

  const diagnostics = result.diagnostics;
  if (diagnostics == null) {
    return {
      succeeded,
      headline,
      meta: result.attempt_outcome ? `outcome ${result.attempt_outcome}` : null,
      summary: null,
      details: null,
    };
  }

  if (typeof diagnostics === "string") {
    const text = readText(diagnostics);
    return {
      succeeded,
      headline,
      meta: null,
      summary: text,
      details: null,
    };
  }

  const record = asRecord(diagnostics) ?? {};
  const meta = collectMeta(record);
  const finalState = record.final_state ?? record.finalState;
  const stdout = extractOutputText(record);
  const fullDump = prettyJson(finalState !== undefined ? finalState : record);

  if (stdout) {
    const stderr = readText(record.stderr);
    const body = stderr ? `${stdout}\n\nstderr:\n${stderr}` : stdout;
    const detailsDistinct = fullDump !== body;
    return {
      succeeded,
      headline,
      meta,
      summary: body,
      details: detailsDistinct ? fullDump : null,
    };
  }

  if (finalState !== undefined) {
    return {
      succeeded,
      headline,
      meta,
      summary: prettyJson(finalState),
      details: null,
    };
  }

  return {
    succeeded,
    headline,
    meta: meta ?? (result.attempt_outcome ? `outcome ${result.attempt_outcome}` : null),
    summary: fullDump,
    details: null,
  };
}

export function formatGraphemeRunResult(
  result: GraphemeRunResponse["result"] | null | undefined,
): string {
  const parsed = parseGraphemeRunResult(result);
  if (!parsed) return "No run result.";

  const lines = [parsed.headline];
  if (parsed.meta) lines.push(parsed.meta);
  if (parsed.summary) {
    lines.push("");
    lines.push(parsed.summary);
  }
  if (parsed.details && parsed.details !== parsed.summary) {
    lines.push("");
    lines.push(parsed.details);
  }
  return lines.join("\n");
}
