import type { ToolArtifactRef, ToolRunState, UiArtifact } from "$lib/types/chat";
import type { ChatMediaAttachment } from "$lib/types/media";

export interface TurnArtifactRef {
  role: string;
  content_type: string;
  byte_size: number;
  hash64: string;
}

export type TurnPart =
  | { kind: "text"; markdown: string }
  | { kind: "progress"; markdown: string }
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
    }
  | {
      kind: "user_media";
      media_id: string;
      mime: string;
      label?: string | null;
      byte_size?: number | null;
    }
  | {
      kind: "attachment_ref";
      artifact_id: string;
      mime: string;
      label: string;
      byte_size?: number | null;
      presentation?: string | null;
      height_px?: number | null;
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
          artifact_id: ref.artifact_id ?? null,
          label: ref.label ?? null,
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

/** Between-tool-round progress notes persisted on assistant turns (not the final answer). */
export function progressFromParts(parts?: TurnPart[] | null): string | null {
  if (!parts?.length) return null;
  const chunks = parts
    .filter((part): part is Extract<TurnPart, { kind: "progress" }> => part.kind === "progress")
    .map((part) => part.markdown)
    .filter((text) => text.trim().length > 0);
  if (chunks.length === 0) return null;
  return chunks[chunks.length - 1] ?? null;
}

export function userMediaFromParts(parts?: TurnPart[] | null): ChatMediaAttachment[] | undefined {
  if (!parts?.length) return undefined;
  const attachments = parts
    .filter((part): part is Extract<TurnPart, { kind: "user_media" }> => part.kind === "user_media")
    .map(
      (part): ChatMediaAttachment => ({
        mediaId: part.media_id,
        kind: part.mime.startsWith("image/") ? "image" : "document",
        mime: part.mime,
        label: part.label?.trim() || part.media_id,
      }),
    );
  return attachments.length > 0 ? attachments : undefined;
}

function normalizePresentation(value?: string | null): UiArtifact["presentation"] {
  const normalized = value?.trim().toLowerCase();
  if (normalized === "panel" || normalized === "fullscreen") {
    return normalized;
  }
  return "inline";
}

export function uiArtifactsFromParts(parts?: TurnPart[] | null): UiArtifact[] | undefined {
  if (!parts?.length) return undefined;
  const artifacts = parts
    .filter((part): part is Extract<TurnPart, { kind: "attachment_ref" }> => part.kind === "attachment_ref")
    .map(
      (part): UiArtifact => ({
        artifactId: part.artifact_id,
        mime: part.mime,
        label: part.label,
        presentation: normalizePresentation(part.presentation),
        byteSize: part.byte_size ?? null,
        heightPx: part.height_px ?? null,
      }),
    );
  return artifacts.length > 0 ? artifacts : undefined;
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
      case "progress":
        if (part.markdown.trim()) {
          sections.push(`> [!note] Progress\n> ${part.markdown.replace(/\n/g, "\n> ")}`);
        }
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
      case "user_media":
        sections.push(
          `> [!note] Attachment: ${part.label ?? "attachment"} (${part.mime})\n> \`media:${part.media_id}\``,
        );
        break;
      case "attachment_ref":
        sections.push(
          `> [!note] Attachment: ${part.label} (${part.mime})\n> \`artifact:${part.artifact_id}\``,
        );
        break;
    }
  }

  return sections.join("\n\n").trim() || content;
}
