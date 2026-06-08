import type { ToolArtifactRef, ToolRunState } from "$lib/types/chat";

export interface TurnArtifactRef {
  role: string;
  content_type: string;
  byte_size: number;
  hash64: string;
}

export type TurnPart =
  | { kind: "text"; markdown: string }
  | { kind: "reasoning"; markdown: string }
  | {
      kind: "tool_run";
      run_id: string;
      tool_name: string;
      status: string;
      input_summary: string;
      output_summary?: string | null;
      artifact_refs?: TurnArtifactRef[];
      tool_round?: number | null;
      started_at?: string;
      finished_at?: string | null;
    }
  | {
      kind: "handoff";
      handoff_kind: string;
      text: string;
      work_id?: string | null;
    };

export function toolRunsFromParts(parts?: TurnPart[] | null): ToolRunState[] | undefined {
  if (!parts?.length) return undefined;

  const runs = parts
    .filter((part): part is Extract<TurnPart, { kind: "tool_run" }> => part.kind === "tool_run")
    .map((part) => ({
      runId: part.run_id,
      toolName: part.tool_name,
      status: part.status === "failed" ? "failed" : part.status === "running" ? "running" : "succeeded",
      round: part.tool_round ?? 1,
      inputSummary: part.input_summary ?? null,
      outputSummary: part.output_summary ?? null,
      artifactRefs: part.artifact_refs?.map(
        (ref): ToolArtifactRef => ({
          role: ref.role,
          content_type: ref.content_type,
          byte_size: ref.byte_size,
          hash64: ref.hash64,
        }),
      ),
    } satisfies ToolRunState));

  return runs.length > 0 ? runs : undefined;
}

export function reasoningFromParts(parts?: TurnPart[] | null): string | null {
  if (!parts?.length) return null;
  const chunks = parts
    .filter((part): part is Extract<TurnPart, { kind: "reasoning" }> => part.kind === "reasoning")
    .map((part) => part.markdown)
    .filter((text) => text.trim().length > 0);
  return chunks.length > 0 ? chunks.join("\n") : null;
}

/** Journal export: Obsidian-flavored markdown from structured parts. */
export function composeTurnMarkdown(
  content: string,
  parts?: TurnPart[] | null,
): string {
  if (!parts?.length) return content;

  const sections: string[] = [];
  for (const part of parts) {
    switch (part.kind) {
      case "text":
        sections.push(part.markdown);
        break;
      case "reasoning":
        if (part.markdown.trim()) {
          sections.push(`> [!abstract] Reasoning\n> ${part.markdown.replace(/\n/g, "\n> ")}`);
        }
        break;
      case "tool_run": {
        let block = `> [!info] Tool: ${part.tool_name} (${part.status})\n> ${part.input_summary}`;
        if (part.output_summary?.trim()) {
          block += `\n> \n> ${part.output_summary}`;
        }
        sections.push(block);
        break;
      }
      case "handoff":
        sections.push(
          `> [!note] Handoff (${part.handoff_kind})\n> ${part.text.replace(/\n/g, "\n> ")}`,
        );
        break;
    }
  }

  return sections.join("\n\n").trim() || content;
}
