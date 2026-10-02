import { listExternalConversations, type ExternalConversation } from "$lib/daemon/externalConversations";

export class ConnectedAgentStore {
  conversations = $state<ExternalConversation[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  workshopScopeId = $state("");
  private sequence = 0;
  private pending: Promise<void> | null = null;
  private loaded = false;
  private refreshAfterPending = false;
  constructor(private readonly list = listExternalConversations) {}
  async refresh(scope: string, force = false): Promise<void> {
    if (scope !== this.workshopScopeId) {
      this.sequence += 1; this.workshopScopeId = scope; this.conversations = []; this.loaded = false; this.pending = null; this.refreshAfterPending = false;
    }
    if (!scope || (this.loaded && !force)) return;
    if (this.pending) { if (force) this.refreshAfterPending = true; return this.pending; }
    const request = ++this.sequence;
    this.loading = true; this.error = null;
    const pending = this.list().then((result) => {
      if (request === this.sequence) {this.conversations = result;this.loaded = true;}
    }).catch((cause) => {
      if (request === this.sequence) this.error = cause instanceof Error ? cause.message : String(cause);
    }).finally(() => {
      if (request !== this.sequence) return;
      this.loading = false; this.pending = null;
      if (this.refreshAfterPending) {
        this.refreshAfterPending = false;
        return this.refresh(scope, true);
      }
    });
    this.pending = pending; return pending;
  }
}
export const connectedAgents = new ConnectedAgentStore();
