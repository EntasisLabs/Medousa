export interface ArtifactCommandResponse {
  selected_context_pack_query: string | null;
  rendered_output: string;
}

export interface ArtifactPreview {
  artifact_id: string;
  rendered_output: string;
  error?: string | null;
}
