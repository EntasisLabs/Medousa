import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";

export interface PeerProposalControls {
  now: number;
  available: boolean;
  busy: boolean;
  busyId: string | null;
  feedback: string | null;
  /** Dispatch failures stay attached to the exact request across refreshes. */
  actionErrors?: Record<string, string>;
  hasMore: boolean;
  loadMore: () => Promise<void>;
  action: (row: PeerProposalReviewRecord, kind: "approve_and_dispatch" | "deny" | "dispatch") => Promise<void>;
  openExecution: (row: PeerProposalReviewRecord, kind: "chat" | "project" | "workshop") => Promise<void>;
}
