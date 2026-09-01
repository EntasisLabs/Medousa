import type { ChatSegment, ToolArtifactRef, ToolRunState, UiArtifact } from "$lib/types/chat";
import type { ChatMediaAttachment } from "$lib/types/media";
import type { HostTurnContext } from "$lib/types/generated/daemon_api";

export interface TurnArtifactRef {
  role: string;
  content_type: string;
  byte_size: number;
  hash64: string;
  artifact_id?: string | null;
  label?: string | null;
}

export type TurnPart =
  | { kind: "model_receipt"; provider: string; model: string }
  | {
      kind: "text";
      markdown: string;
      segment_id?: string | null;
      model_round?: number | null;
    }
  | { kind: "progress"; markdown: string }
  | { kind: "reasoning"; markdown: string }
  | {
      kind: "tool_run";
      run_id: string;
      tool_name: string;
      status: string;
      input_summary: string;
      input_params?: import("$lib/types/card").ToolInputParam[];
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
      kind: "host_context";
      context: HostTurnContext;
    }
  | {
      kind: "attachment_ref";
      artifact_id: string;
      mime: string;
      label: string;
      byte_size?: number | null;
      presentation?: string | null;
      height_px?: number | null;
    }
  | { kind: "unknown" };

export function modelReceiptFromParts(
  parts?: TurnPart[] | null,
): { provider: string; model: string } | null {
  if (!parts?.length) return null;
  const receipt = parts.find(
    (part): part is Extract<TurnPart, { kind: "model_receipt" }> =>
      part.kind === "model_receipt",
  );
  if (!receipt?.provider.trim() || !receipt.model.trim()) return null;
  return { provider: receipt.provider.trim(), model: receipt.model.trim() };
}

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
      inputParams: part.input_params,
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

/**
 * Project native chronological parts into Medousa's segment model.
 *
 * Legacy turns intentionally return `undefined`: their grouped parts cannot
 * prove where prose occurred relative to tools, so the legacy layout remains
 * preferable to fabricated chronology.
 */
export function chatSegmentsFromParts(parts?: TurnPart[] | null): ChatSegment[] | undefined {
  if (!parts?.length) return undefined;
  const textParts = parts.filter(
    (part): part is Extract<TurnPart, { kind: "text" }> => part.kind === "text",
  );
  if (!textParts.length || textParts.some((part) => !part.segment_id?.trim())) {
    return undefined;
  }

  const segments: ChatSegment[] = [];
  for (const [partIndex, part] of parts.entries()) {
    switch (part.kind) {
      case "text": {
        const segmentId = part.segment_id?.trim();
        if (!segmentId) break;
        segments.push({
          kind: "text",
          segmentId,
          modelRound: part.model_round ?? null,
          markdown: part.markdown,
          committed: true,
        });
        break;
      }
      case "progress":
        if (part.markdown.trim()) {
          segments.push({
            kind: "progress",
            progressId: `history-progress:${partIndex}`,
            markdown: part.markdown.trim(),
          });
        }
        break;
      case "tool_run": {
        const run: ToolRunState = {
          runId: part.run_id,
          toolName: part.tool_name,
          status:
            part.status === "failed"
              ? "failed"
              : part.status === "running"
                ? "running"
                : "succeeded",
          round: part.tool_round ?? 1,
          inputSummary: part.input_summary ?? null,
          inputParams: part.input_params,
          outputSummary: part.output_summary ?? null,
          artifactRefs: part.artifact_refs?.map((ref) => ({
            role: ref.role,
            content_type: ref.content_type,
            byte_size: ref.byte_size,
            hash64: ref.hash64,
            artifact_id: ref.artifact_id ?? null,
            label: ref.label ?? null,
          })),
        };
        const previous = segments.at(-1);
        if (previous?.kind === "tool_group") {
          previous.runs.push(run);
        } else {
          segments.push({
            kind: "tool_group",
            groupId: `history-tool-group:${part.run_id}`,
            toolRound: run.round,
            runs: [run],
          });
        }
        break;
      }
      case "attachment_ref":
        segments.push({
          kind: "artifact",
          artifact: {
            artifactId: part.artifact_id,
            mime: part.mime,
            label: part.label,
            presentation: normalizePresentation(part.presentation),
            byteSize: part.byte_size ?? null,
            heightPx: part.height_px ?? null,
            rootArtifactId: null,
          },
        });
        break;
      case "handoff":
        segments.push({
          kind: "handoff",
          handoffKind: part.handoff_kind,
          text: part.text,
          workId: part.work_id ?? null,
        });
        break;
      default:
        break;
    }
  }
  return segments.length > 0 ? segments : undefined;
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

export function hostContextFromParts(parts?: TurnPart[] | null): HostTurnContext | null {
  if (!parts?.length) return null;
  return parts.find(
    (part): part is Extract<TurnPart, { kind: "host_context" }> => part.kind === "host_context",
  )?.context ?? null;
}

export function hostContextLabel(context?: HostTurnContext | null): string | null {
  if (!context) return null;
  const source = ({
    vscode: "VS Code",
    neovim: "Neovim",
    obsidian: "Obsidian",
    browser: "Browser",
  } as Record<string, string>)[context.source.toLowerCase()] ?? context.source;
  const resource = context.resource_path?.split(/[\\/]/).filter(Boolean).at(-1)
    ?? context.resource_title
    ?? (context.resource_url ? (() => {
      try { return new URL(context.resource_url).hostname; } catch { return context.resource_url; }
    })() : null);
  const selection = context.selection?.start
    ? context.selection.end && context.selection.end.line !== context.selection.start.line
      ? `lines ${context.selection.start.line + 1}–${context.selection.end.line + 1}`
      : `line ${context.selection.start.line + 1}`
    : null;
  const diagnosticCount = context.diagnostics?.length ?? 0;
  const diagnostics = diagnosticCount > 0
    ? `${diagnosticCount} diagnostic${diagnosticCount === 1 ? "" : "s"}`
    : null;
  return [source, resource, selection, diagnostics].filter(Boolean).join(" · ");
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
      case "host_context":
      case "unknown":
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
