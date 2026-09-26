import { daemonUnary } from "./contractClient";
import type { PeerAssignmentProposal, PeerProposalActionResponse, PeerProposalInboxResponse } from "$lib/types/generated/daemon_api";

/** Exact-runtime transport resolves paired portals only, not the local daemon. */
export function proposalExecutionTransport(kind: string | undefined, runtimeId: string): string | null {
  return kind === "portal" || kind === "paired" ? runtimeId : null;
}

export function listPeerProposals(sessionId: string, runtimeId?: string | null, after?: string): Promise<PeerProposalInboxResponse> {
  return daemonUnary("coordination.proposals.get", {}, undefined, runtimeId, {
    session_id: sessionId, ...(after ? { after } : {}),
  });
}

/** Only the immutable proposal id is submitted. Never send grants or revised instructions. */
export function actOnPeerProposal(proposal: PeerAssignmentProposal, action: "approve" | "deny" | "dispatch", runtimeId?: string | null): Promise<PeerProposalActionResponse> {
  const operations = {
    approve: "coordination.channels.by_channel_id.proposals.by_proposal_id.approve.post",
    deny: "coordination.channels.by_channel_id.proposals.by_proposal_id.deny.post",
    dispatch: "coordination.channels.by_channel_id.proposals.by_proposal_id.dispatch.post",
  } as const;
  return daemonUnary(operations[action], {
    channel_id: proposal.request.channel.channel_id, proposal_id: proposal.proposal_id,
  }, {}, runtimeId);
}
