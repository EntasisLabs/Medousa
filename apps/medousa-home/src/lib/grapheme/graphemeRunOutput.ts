import type { GraphemeRunResponse } from "$lib/types/grapheme";

const OUTPUT_KEYS = [
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

function extractOutputText(payload: unknown): string | null {
  if (payload == null) return null;
  if (typeof payload === "string") return readText(payload);

  if (typeof payload !== "object") return null;
  const record = payload as Record<string, unknown>;

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

  for (const key of ["result", "response", "output", "final", "completion"]) {
    const nested = record[key];
    if (nested && typeof nested === "object") {
      const text = extractOutputText(nested);
      if (text) return text;
    }
  }

  return readText(JSON.stringify(record));
}

export function formatGraphemeRunResult(
  result: GraphemeRunResponse["result"] | null | undefined,
): string {
  if (!result) return "No run result.";

  const succeeded = result.succeeded ?? false;
  const lines = [succeeded ? "Run succeeded" : "Run failed"];

  const diagnostics = result.diagnostics;
  if (diagnostics == null) {
    if (result.attempt_outcome) {
      lines.push(`Outcome: ${result.attempt_outcome}`);
    }
    return lines.join("\n");
  }

  if (typeof diagnostics === "string") {
    const text = readText(diagnostics);
    if (text) {
      lines.push("");
      lines.push(text);
    }
    return lines.join("\n");
  }

  const record = diagnostics as Record<string, unknown>;
  const finalState = record.final_state ?? record.finalState;
  if (finalState !== undefined) {
    lines.push("");
    lines.push(JSON.stringify(finalState, null, 2));
    return lines.join("\n");
  }

  const outputText = extractOutputText(record);
  if (outputText) {
    lines.push("");
    lines.push(outputText);
    return lines.join("\n");
  }

  lines.push("");
  lines.push(JSON.stringify(record, null, 2));

  if (!succeeded && result.attempt_outcome) {
    lines.push("");
    lines.push(`Outcome: ${result.attempt_outcome}`);
  }

  return lines.join("\n");
}
