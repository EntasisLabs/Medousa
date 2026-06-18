import { getIdentityContext } from "$lib/daemon";
import type { IdentityContextResponse } from "$lib/types/identity";

export class IdentityStore {
  context = $state<IdentityContextResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  async refresh(options?: { relationshipLimit?: number }) {
    this.loading = true;
    this.error = null;
    try {
      this.context = await getIdentityContext({
        mode: "cognitive",
        relationship_limit: options?.relationshipLimit ?? 24,
      });
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      this.context = null;
    } finally {
      this.loading = false;
    }
  }

  /** @deprecated Session id is not an identity principal; use refresh(). */
  async refreshForSession(_sessionId: string) {
    await this.refresh({ relationshipLimit: 8 });
  }

  clear() {
    this.context = null;
    this.error = null;
  }
}

export const identity = new IdentityStore();
