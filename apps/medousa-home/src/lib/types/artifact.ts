export interface ArtifactCommandResponse {
  selected_context_pack_query: string | null;
  rendered_output: string;
}

export interface ArtifactFetchResponse {
  artifact_id: string;
  mime: string;
  label: string;
  body: string;
  byte_size: number;
  presentation?: string | null;
  height_px?: number | null;
  payload_path?: string | null;
}

export interface ArtifactSummary {
  artifact_id: string;
  session_id: string;
  label: string;
  presentation?: string | null;
  byte_size: number;
  stored_at_utc: string;
  root_artifact_id?: string | null;
  supersedes_artifact_id?: string | null;
}

export interface ArtifactListUiResponse {
  artifacts: ArtifactSummary[];
}

export interface ArtifactWriteRequest {
  session_id: string;
  artifact_id: string;
  title: string;
  html: string;
  presentation?: string | null;
  height_px?: number | null;
  if_match_hash64?: string | null;
}

export interface ArtifactWriteResponse {
  artifact_id: string;
  supersedes_artifact_id: string;
  hash64: string;
  label: string;
}

export interface ArtifactDeleteRequest {
  session_id: string;
  artifact_id: string;
}

export interface ArtifactDeleteResponse {
  deleted_artifact_ids: string[];
}

export interface ArtifactPreview {
  artifact_id: string;
  rendered_output: string;
  error?: string | null;
}

export type UiArtifactPresentation = "inline" | "panel" | "fullscreen";

export function artifactSummaryToUi(artifact: ArtifactSummary): {
  artifactId: string;
  mime: string;
  label: string;
  presentation: UiArtifactPresentation;
  byteSize: number | null;
  heightPx: number | null;
} {
  return {
    artifactId: artifact.artifact_id,
    mime: "text/html",
    label: artifact.label,
    presentation:
      artifact.presentation === "panel" || artifact.presentation === "fullscreen"
        ? artifact.presentation
        : "inline",
    byteSize: artifact.byte_size,
    heightPx: null,
  };
}

export function mapStreamUiArtifact(
  artifact: {
    artifact_id: string;
    mime: string;
    label: string;
    presentation: string;
    byte_size?: number | null;
    height_px?: number | null;
  },
): {
  artifactId: string;
  mime: string;
  label: string;
  presentation: UiArtifactPresentation;
  byteSize: number | null;
  heightPx: number | null;
} {
  return {
    artifactId: artifact.artifact_id,
    mime: artifact.mime,
    label: artifact.label,
    presentation:
      artifact.presentation === "panel" || artifact.presentation === "fullscreen"
        ? artifact.presentation
        : "inline",
    byteSize: artifact.byte_size ?? null,
    heightPx: artifact.height_px ?? null,
  };
}

export function replaceUiArtifactEntry<
  T extends { artifactId: string; rootArtifactId?: string | null },
>(
  entries: T[],
  previousArtifactId: string,
  rootArtifactId: string | null | undefined,
  next: T,
): T[] {
  let replaced = false;
  const mapped = entries.map((entry) => {
    const matchesPrevious = entry.artifactId === previousArtifactId;
    const matchesRoot =
      rootArtifactId != null &&
      entry.rootArtifactId != null &&
      entry.rootArtifactId === rootArtifactId;
    if (matchesPrevious || matchesRoot) {
      replaced = true;
      return next;
    }
    return entry;
  });
  return replaced ? mapped : [...mapped, next];
}
