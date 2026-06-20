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

export interface ParsedGraphemeRunResult {
  succeeded: boolean;
  headline: string;
  summary: string | null;
  details: string | null;
}

export function parseGraphemeRunResult(
  result: GraphemeRunResponse["result"] | null | undefined,
): ParsedGraphemeRunResult | null {
  if (!result) return null;

  const succeeded = result.succeeded ?? false;
  const headline = succeeded ? "It worked" : "Something went wrong";

  const diagnostics = result.diagnostics;
  if (diagnostics == null) {
    return {
      succeeded,
      headline,
      summary: succeeded
        ? "Your script finished without errors."
        : result.attempt_outcome ?? "The run did not complete.",
      details: result.attempt_outcome ? `Outcome: ${result.attempt_outcome}` : null,
    };
  }

  if (typeof diagnostics === "string") {
    const text = readText(diagnostics);
    return {
      succeeded,
      headline,
      summary: text ?? (succeeded ? "Run completed." : "Run failed."),
      details: null,
    };
  }

  const record = diagnostics as Record<string, unknown>;
  const finalState = record.final_state ?? record.finalState;
  const outputText = extractOutputText(record);

  if (outputText) {
    return {
      succeeded,
      headline,
      summary: outputText.length > 280 ? `${outputText.slice(0, 277)}…` : outputText,
      details:
        finalState !== undefined
          ? JSON.stringify(finalState, null, 2)
          : JSON.stringify(record, null, 2),
    };
  }

  if (finalState !== undefined) {
    const pretty = JSON.stringify(finalState, null, 2);
    const firstLine = pretty.split("\n").find((line) => line.trim()) ?? pretty;
    return {
      succeeded,
      headline,
      summary:
        firstLine.length > 120
          ? "Your script returned structured data — expand for details."
          : firstLine.trim(),
      details: pretty,
    };
  }

  const pretty = JSON.stringify(record, null, 2);
  return {
    succeeded,
    headline,
    summary: succeeded
      ? "Your script finished. Expand technical details below."
      : "The run failed. Expand technical details below.",
    details: pretty,
  };
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
