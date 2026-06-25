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
}

export interface ArtifactPreview {
  artifact_id: string;
  rendered_output: string;
  error?: string | null;
}
