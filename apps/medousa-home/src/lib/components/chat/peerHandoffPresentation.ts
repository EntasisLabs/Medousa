import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import { peerProgressPresentation } from "./peerProgress";

/** Worker completion is evidence; the sender's review controls acceptance. */
export function peerHandoffPresentation(row: PeerProposalReviewRecord, now: number) {
  const progress = peerProgressPresentation(row, now);
  const handoff = row.handoff;
  const needsApproval = !row.binding && !row.receipt && handoff?.admission !== "delegate";
  let status = progress.headline;
  if (row.receipt && row.receipt.outcome !== "completed") status = progress.headline;
  else if (handoff?.state === "awaiting_sender_review") status = "Awaiting sender review";
  else if (handoff?.state === "changes_requested") status = "Changes requested";
  else if (handoff?.state === "failed") status = "Work failed";
  else if (handoff?.state === "accepted") status = handoff.review ? "Result accepted" : "Work completed";
  else if (needsApproval) status = row.decision?.approved ? "Approved · ready to start" : "Needs your approval";
  else if (handoff?.state === "awaiting_acceptance") status = "Waiting for agent";
  else if (row.progress?.state === "blocked") status = "Needs attention";
  else if (row.progress?.state === "running" || handoff?.state === "working") status = "Agent working";
  else if (row.progress?.state === "accepted") status = "Agent accepted";

  const senderResponsible = handoff
    ? handoff.responsible_session.authority_id === row.proposal.request.owner_session.authority_id
      && handoff.responsible_session.session_id === row.proposal.request.owner_session.session_id
    : true;
  const senderStatus = !handoff ? "Requested this work"
    : handoff.state === "awaiting_sender_review" ? "Review pending"
    : handoff.state === "changes_requested" ? "Requested changes"
    : handoff.review?.verdict === "accept" ? "Reviewed and accepted"
    : (handoff.policy.completion ?? "sender_review") === "sender_review" ? "Will review the result"
    : "Worker result completes this request";
  return {
    ...progress, status, needsApproval, senderResponsible, senderStatus,
    workerStatus: row.receipt ? progress.headline
      : row.progress?.state === "accepted" ? "Accepted"
      : row.progress?.state === "blocked" ? "Needs attention"
      : row.progress?.state === "awaiting_receipt" ? "Awaiting result"
      : row.progress?.state === "unobserved" ? "Status unavailable"
      : row.progress?.state === "running" || handoff?.state === "working" ? "Working"
      : row.binding ? "Assigned" : "Not started",
    attention: row.progress?.state === "blocked" || handoff?.state === "changes_requested"
      || Boolean(row.receipt && row.receipt.outcome !== "completed"),
  };
}

/** A title-sized fallback for workshops whose project is not loaded in Home. */
export function peerWorkTitle(instructions: string, projectTitle?: string | null): string {
  const firstLine = instructions.split(/\r?\n/).find(line => line.trim())?.trim() || "Delegated work";
  const undertaking = firstLine.match(/undertaking\s+["“]([^"”]+)["”]/i)?.[1];
  if (undertaking) return undertaking;
  if (firstLine.length > 80 && projectTitle?.trim()) return projectTitle.trim();
  return firstLine.length > 80 ? `${firstLine.slice(0, 77).trimEnd()}…` : firstLine;
}
