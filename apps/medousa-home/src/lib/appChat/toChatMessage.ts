import type { ChatMessage, ChatSegment, ToolRunState, UiArtifact } from "$lib/types/chat";
import type {
  ChatItem,
  ChatSegment as AppChatSegment,
  ChatSnapshot,
  ToolRunState as AppToolRunState,
  UiArtifact as AppUiArtifact,
} from "$lib/types/generated/app_api";

function mapToolRun(run: AppToolRunState): ToolRunState {
  return {
    runId: run.runId,
    toolName: run.toolName,
    status:
      run.status === "failed" ? "failed" : run.status === "running" ? "running" : "succeeded",
    round: run.round,
    inputSummary: run.inputSummary ?? null,
    inputParams: run.inputParams ?? undefined,
    outputSummary: run.outputSummary ?? null,
    artifactRefs: run.artifactRefs?.map((ref) => ({
      role: ref.role,
      content_type: ref.contentType,
      byte_size: ref.byteSize,
      hash64: ref.hash64,
      artifact_id: ref.artifactId ?? null,
      label: ref.label ?? null,
    })),
  };
}

function mapSegment(segment: AppChatSegment): ChatSegment {
  switch (segment.kind) {
    case "text":
      return {
        kind: "text",
        segmentId: segment.segmentId,
        modelRound: segment.modelRound ?? null,
        markdown: segment.markdown,
        committed: segment.committed,
      };
    case "progress":
      return {
        kind: "progress",
        progressId: segment.progressId,
        markdown: segment.markdown,
      };
    case "tool_group":
      return {
        kind: "tool_group",
        groupId: segment.groupId,
        toolRound: segment.toolRound,
        runs: segment.runs.map(mapToolRun),
      };
    case "artifact":
      return { kind: "artifact", artifact: mapArtifact(segment.artifact) };
    case "handoff":
      return {
        kind: "handoff",
        handoffKind: segment.handoffKind,
        text: segment.text,
        workId: segment.workId ?? null,
      };
  }
}

function mapArtifact(artifact: AppUiArtifact): UiArtifact {
  return {
    artifactId: artifact.artifactId,
    mime: artifact.mime,
    label: artifact.label,
    presentation: artifact.presentation,
    byteSize: artifact.byteSize ?? null,
    heightPx: artifact.heightPx ?? null,
    rootArtifactId: artifact.rootArtifactId ?? null,
  };
}

/** Map a Rust application-core item into the renderer ChatMessage shape. */
export function chatItemToMessage(item: ChatItem): ChatMessage {
  return {
    id: item.id,
    role: item.role,
    content: item.content,
    createdAt: item.createdAt ?? undefined,
    streaming: item.streaming ?? undefined,
    turnId: item.turnId ?? undefined,
    turnIndex: item.turnIndex ?? undefined,
    lane: item.lane ?? undefined,
    askJobId: item.askJobId ?? undefined,
    workId: item.workId ?? undefined,
    phase: item.phase ?? null,
    statusLine: item.statusLine ?? null,
    tools: item.tools ?? undefined,
    toolRuns: item.toolRuns?.map(mapToolRun),
    segments: item.segments?.map(mapSegment),
    uiArtifacts: item.uiArtifacts?.map(mapArtifact),
    reasoning: item.reasoning ?? undefined,
    answerState: item.answerState ?? undefined,
    responseProvider: item.responseProvider ?? undefined,
    responseModel: item.responseModel ?? undefined,
    stageWhisper: item.stageWhisper ?? undefined,
    budgetRequestId: item.budgetRequestId ?? undefined,
    requestedRounds: item.requestedRounds ?? undefined,
    failed: item.failed ?? undefined,
    errorLine: item.errorLine ?? undefined,
    errorDetail: item.errorDetail ?? undefined,
    speakerProfileId: item.speakerProfileId ?? undefined,
    mediaAttachments: item.mediaAttachments?.map((media) => ({
      mediaId: media.mediaId,
      kind: media.kind as "image" | "document" | "drawing",
      mime: media.mime,
      label: media.label ?? "",
      origin: media.origin as "user" | "drawing" | "generated" | undefined,
      editableSourceId: media.editableSourceId ?? undefined,
      generationId: media.generationId ?? undefined,
      parentGenerationId: media.parentGenerationId ?? undefined,
      widthPx: media.widthPx ?? undefined,
      heightPx: media.heightPx ?? undefined,
    })),
    hostContext: item.hostContext ?? undefined,
    reactions: item.reactions ?? undefined,
    liveTranscripts: item.liveTranscripts?.map((attachment) => ({
      id: attachment.id,
      rows: attachment.rows.map((row) => ({
        role: row.role === "system" ? "assistant" : row.role,
        text: row.text,
      })),
    })),
    transcript: item.transcript
      ? {
          authorityId: item.transcript.authorityId,
          sessionId: item.transcript.sessionId,
          entryId: item.transcript.entryId,
          entrySeq: item.transcript.entrySeq,
          source: item.transcript.source
            ? {
                authorityId: item.transcript.source.authorityId,
                sessionId: item.transcript.source.sessionId,
                entryId: item.transcript.source.entryId,
                entrySeq: item.transcript.source.entrySeq,
              }
            : null,
        }
      : null,
  };
}

export function snapshotMessages(snapshot: ChatSnapshot): ChatMessage[] {
  return snapshot.items.map(chatItemToMessage);
}

/** Apply snapshot fields the store owns for the focused session. */
export function applySnapshotToMessages(snapshot: ChatSnapshot): {
  messages: ChatMessage[];
  activeTurnId: string | null;
  lastSeqByTurn: Map<string, number>;
  revision: number;
} {
  const lastSeqByTurn = new Map<string, number>();
  for (const [turnId, seq] of Object.entries(snapshot.lastSeqByTurn ?? {})) {
    lastSeqByTurn.set(turnId, seq);
  }
  return {
    messages: snapshotMessages(snapshot),
    activeTurnId: snapshot.activeTurnId ?? null,
    lastSeqByTurn,
    revision: snapshot.revision,
  };
}

/** Merge Rust worker-lane facts into the host WorkerLink map (preserves sessionId). */
export function applySnapshotWorkers(
  existing: Map<string, import("$lib/chat/chatSessionRuntime").WorkerLink>,
  snapshot: ChatSnapshot,
  fallbackSessionId: string,
): Map<string, import("$lib/chat/chatSessionRuntime").WorkerLink> {
  if (!snapshot.workers?.length) return existing;
  const next = new Map(existing);
  for (const fact of snapshot.workers) {
    const prior = next.get(fact.workId);
    next.set(fact.workId, {
      workId: fact.workId,
      parentTurnId: fact.parentTurnId ?? prior?.parentTurnId ?? null,
      messageId: fact.parentItemId ?? prior?.messageId ?? null,
      synthesisMessageId: fact.synthesisItemId ?? prior?.synthesisMessageId ?? null,
      sessionId: prior?.sessionId ?? fallbackSessionId,
      synthesisDelivered: fact.synthesisDelivered || prior?.synthesisDelivered || false,
    });
  }
  return next;
}
