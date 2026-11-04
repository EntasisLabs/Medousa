import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";

const api = vi.hoisted(() => ({
  open: vi.fn(async () => ({ sessionId: "ses", authorityId: "auth", items: [], revision: 1 })),
  apply: vi.fn(async () => ({
    sessionId: "ses",
    authorityId: "auth",
    items: [],
    revision: 2,
    proposals: [{ proposal_id: "p1" }],
  })),
}));

vi.mock("$lib/appChat/client", () => ({
  openChatSession: api.open,
  applyChatProposals: api.apply,
}));

import {
  commitChatProposals,
  resetCommitChatProposalsMemo,
} from "./dualWriteProposals";

function row(id: string): PeerProposalReviewRecord {
  const authority = `auth_${"a".repeat(64)}`;
  return {
    proposal: {
      proposal_id: id,
      expires_at: "2026-11-04T01:00:00Z",
      continue_owner: false,
      request: {
        assignment_id: id,
        idempotency_key: id,
        owner_principal_id: "owner",
        owner_session: { authority_id: authority, session_id: "ses_owner" },
        execution_session: { authority_id: authority, session_id: "ses_exec" },
        execution_grant_id: "grant",
        instructions: "go",
        target: {
          runtime: "medousa",
          execution_runtime_id: "local",
          authority_id: authority,
        },
        context: {
          manifest_id: "ctx",
          created_by: "owner",
          created_at: "2026-11-04T00:00:00Z",
          sources: [],
        },
        channel: { authority_id: authority, channel_id: "ch" },
        forge_work_id: "work",
      },
    },
    decision: null,
    binding: null,
    progress: null,
  } as PeerProposalReviewRecord;
}

describe("commitChatProposals", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetCommitChatProposalsMemo();
  });

  it("opens the session, applies thin proposal facts, and paints", async () => {
    const paint = vi.fn();
    commitChatProposals("ses_owner", [row("p1")], {
      authorityId: `auth_${"a".repeat(64)}`,
      paint,
    });
    await vi.waitFor(() => expect(api.apply).toHaveBeenCalled());
    expect(api.open).toHaveBeenCalledWith("ses_owner", `auth_${"a".repeat(64)}`);
    expect(api.apply).toHaveBeenCalledWith(
      "ses_owner",
      [
        expect.objectContaining({
          proposal_id: "p1",
          assignment_id: "p1",
        }),
      ],
    );
    await vi.waitFor(() => expect(paint).toHaveBeenCalled());
    expect(paint).toHaveBeenCalledWith(
      "ses_owner",
      expect.objectContaining({ revision: 2 }),
    );
  });

  it("skips identical fact vectors", async () => {
    commitChatProposals("ses_owner", [row("p1")]);
    await vi.waitFor(() => expect(api.apply).toHaveBeenCalledTimes(1));
    commitChatProposals("ses_owner", [row("p1")]);
    await Promise.resolve();
    expect(api.apply).toHaveBeenCalledTimes(1);
  });

  it("clears core proposals when the inbox is empty", async () => {
    commitChatProposals("ses_owner", [row("p1")]);
    await vi.waitFor(() => expect(api.apply).toHaveBeenCalledTimes(1));
    commitChatProposals("ses_owner", []);
    await vi.waitFor(() => expect(api.apply).toHaveBeenCalledTimes(2));
    expect(api.apply).toHaveBeenLastCalledWith("ses_owner", []);
  });

  it("no-ops without a session id", () => {
    commitChatProposals(null, [row("p1")]);
    expect(api.open).not.toHaveBeenCalled();
    expect(api.apply).not.toHaveBeenCalled();
  });
});
