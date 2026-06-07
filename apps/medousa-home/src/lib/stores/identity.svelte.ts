import { getIdentityContext } from "$lib/daemon";
import type { IdentityContextResponse } from "$lib/types/identity";

export class IdentityStore {
  context = $state<IdentityContextResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  async refreshForSession(sessionId: string) {
    this.loading = true;
    this.error = null;
    try {
      this.context = await getIdentityContext({
        channel_id: sessionId,
        mode: "cognitive",
        relationship_limit: 8,
      });
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      this.context = null;
    } finally {
      this.loading = false;
    }
  }

  clear() {
    this.context = null;
    this.error = null;
  }
}

export const identity = new IdentityStore();
