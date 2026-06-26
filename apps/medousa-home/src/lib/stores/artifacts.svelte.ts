import { listUiArtifacts } from "$lib/daemon";
import { chat } from "$lib/stores/chat.svelte";
import type { ArtifactSummary } from "$lib/types/artifact";

export class ArtifactsStore {
  artifacts = $state<ArtifactSummary[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  selectedArtifactId = $state<string | null>(null);
  searchQuery = $state("");

  selectedArtifact = $derived.by(() => {
    const id = this.selectedArtifactId;
    if (!id) return null;
    return this.artifacts.find((artifact) => artifact.artifact_id === id) ?? null;
  });

  filteredArtifacts = $derived.by(() => {
    const query = this.searchQuery.trim().toLowerCase();
    if (!query) return this.artifacts;
    return this.artifacts.filter((artifact) => {
      const haystack = `${artifact.label} ${artifact.artifact_id} ${artifact.session_id}`.toLowerCase();
      return haystack.includes(query);
    });
  });

  sessionTitle(sessionId: string): string {
    const match = chat.sessions.find((session) => session.session_id === sessionId);
    return match?.display_name?.trim() || sessionId;
  }

  async refresh(options?: { sessionId?: string; query?: string }) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listUiArtifacts({
        sessionId: options?.sessionId,
        query: options?.query,
        limit: 100,
      });
      this.artifacts = response.artifacts;
      if (
        this.selectedArtifactId &&
        !this.artifacts.some((artifact) => artifact.artifact_id === this.selectedArtifactId)
      ) {
        this.selectedArtifactId = this.artifacts[0]?.artifact_id ?? null;
      } else if (!this.selectedArtifactId && this.artifacts.length > 0) {
        this.selectedArtifactId = this.artifacts[0].artifact_id;
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  selectArtifact(artifactId: string) {
    this.selectedArtifactId = artifactId;
  }

  setSearchQuery(query: string) {
    this.searchQuery = query;
  }
}

export const artifacts = new ArtifactsStore();
