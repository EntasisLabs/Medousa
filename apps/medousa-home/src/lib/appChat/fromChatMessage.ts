import type { ChatMessage, ChatSegment, ToolRunState, UiArtifact } from "$lib/types/chat";
import type {
  ChatItem,
  ChatSegment as AppChatSegment,
  ToolRunState as AppToolRunState,
  UiArtifact as AppUiArtifact,
} from "$lib/types/generated/app_api";

function mapToolRun(run: ToolRunState): AppToolRunState {
  return {
    runId: run.runId,
    toolName: run.toolName,
    status: run.status,
    round: run.round,
    inputSummary: run.inputSummary ?? null,
    inputParams: run.inputParams ?? null,
    outputSummary: run.outputSummary ?? null,
    artifactRefs:
      run.artifactRefs?.map((ref) => ({
        role: ref.role,
        contentType: ref.content_type,
        byteSize: ref.byte_size,
        hash64: ref.hash64,
        artifactId: ref.artifact_id ?? null,
        label: ref.label ?? null,
      })) ?? null,
  };
}

function mapSegment(segment: ChatSegment): AppChatSegment {
  switch (segment.kind) {
    case "text":
      return {
        kind: "text",
        segmentId: segment.segmentId,
        modelRound: segment.modelRound,
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

function mapArtifact(artifact: UiArtifact): AppUiArtifact {
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

/** Seed the Rust core with renderer-held optimistic / streaming rows. */
export function chatMessageToItem(message: ChatMessage): ChatItem {
  return {
    id: message.id,
    role: message.role,
    content: message.content,
    createdAt: message.createdAt ?? null,
    streaming: message.streaming ?? null,
    turnId: message.turnId ?? null,
    turnIndex: message.turnIndex ?? null,
    lane: message.lane ?? null,
    askJobId: message.askJobId ?? null,
    workId: message.workId ?? null,
    phase: message.phase ?? null,
    statusLine: message.statusLine ?? null,
    tools: message.tools ?? null,
    toolRuns: message.toolRuns?.map(mapToolRun) ?? null,
    segments: message.segments?.map(mapSegment) ?? null,
    uiArtifacts: message.uiArtifacts?.map(mapArtifact) ?? null,
    reasoning: message.reasoning ?? null,
    answerState: message.answerState ?? null,
    responseProvider: message.responseProvider ?? null,
    responseModel: message.responseModel ?? null,
    stageWhisper: message.stageWhisper ?? null,
    budgetRequestId: message.budgetRequestId ?? null,
    requestedRounds: message.requestedRounds ?? null,
    failed: message.failed ?? null,
    errorLine: message.errorLine ?? null,
    errorDetail: message.errorDetail ?? null,
    speakerProfileId: message.speakerProfileId ?? null,
    mediaAttachments:
      message.mediaAttachments?.map((media) => ({
        mediaId: media.mediaId,
        kind: media.kind,
        mime: media.mime,
        label: media.label ?? null,
        origin: media.origin ?? null,
        editableSourceId: media.editableSourceId ?? null,
        generationId: media.generationId ?? null,
        parentGenerationId: media.parentGenerationId ?? null,
        widthPx: media.widthPx ?? null,
        heightPx: media.heightPx ?? null,
      })) ?? null,
    hostContext: message.hostContext ?? null,
    reactions: message.reactions ?? null,
    liveTranscripts:
      message.liveTranscripts?.map((attachment) => ({
        id: attachment.id,
        rows: attachment.rows.map((row) => ({
          role: row.role,
          text: row.text,
        })),
      })) ?? null,
    transcript: message.transcript
      ? {
          authorityId: message.transcript.authorityId,
          sessionId: message.transcript.sessionId,
          entryId: message.transcript.entryId,
          entrySeq: message.transcript.entrySeq,
          source: message.transcript.source
            ? {
                authorityId: message.transcript.source.authorityId,
                sessionId: message.transcript.source.sessionId,
                entryId: message.transcript.source.entryId,
                entrySeq: message.transcript.source.entrySeq,
              }
            : null,
        }
      : null,
  };
}
