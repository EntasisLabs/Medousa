import { getIdentityContext } from "$lib/daemon";
import type { IdentityContextResponse } from "$lib/types/identity";

export class IdentityStore {
  context = $state<IdentityContextResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  private refreshEpoch = 0;

  async refresh(options?: { relationshipLimit?: number; userId?: string | null }) {
    const requestEpoch = ++this.refreshEpoch;
    this.loading = true;
    this.error = null;
    try {
      const context = await getIdentityContext({
        mode: "cognitive",
        relationship_limit: options?.relationshipLimit ?? 24,
        user_id: options?.userId ?? undefined,
      });
      if (requestEpoch !== this.refreshEpoch) return;
      this.context = context;
    } catch (err) {
      if (requestEpoch !== this.refreshEpoch) return;
      this.error = err instanceof Error ? err.message : String(err);
      this.context = null;
    } finally {
      if (requestEpoch === this.refreshEpoch) {
        this.loading = false;
      }
    }
  }

  /** @deprecated Session id is not an identity principal; use refresh(). */
  async refreshForSession(_sessionId: string) {
    await this.refresh({ relationshipLimit: 8 });
  }

  clear() {
    this.refreshEpoch += 1;
    this.context = null;
    this.loading = false;
    this.error = null;
  }
}

export const identity = new IdentityStore();
