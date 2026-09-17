import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ unary: vi.fn() }));
vi.mock("./contractClient", () => ({ daemonUnary: (...args: unknown[]) => mocks.unary(...args) }));
import { actOnPeerProposal, listPeerProposals, proposalExecutionTransport } from "./coordination";
import type { PeerAssignmentProposal } from "$lib/types/generated/daemon_api";

describe("operator coordination client", () => {
  it("does not incorrectly route a local daemon through the paired-runtime resolver", () => {
    expect(proposalExecutionTransport("local", "daemon-one")).toBeNull();
    expect(proposalExecutionTransport("portal", "daemon-one")).toBe("daemon-one");
    expect(proposalExecutionTransport("paired", "daemon-one")).toBe("daemon-one");
  });
  beforeEach(() => mocks.unary.mockReset());
  it("scopes the inbox to the owner session and preserves pagination and workshop pin", async () => {
    mocks.unary.mockResolvedValue({ proposals: [], next_cursor: null });
    await listPeerProposals("session-one", "workshop-one", "proposal-cursor");
    expect(mocks.unary).toHaveBeenCalledWith("coordination.proposals.get", {}, undefined, "workshop-one", { session_id: "session-one", after: "proposal-cursor" });
  });
  it.each(["approve", "deny", "dispatch"] as const)("%s submits only the persisted identity, not client instructions or grants", async action => {
    const proposal = { proposal_id: "immutable-proposal", request: { channel: { channel_id: "channel/one" }, instructions: "must not be sent", owner_principal_id: "cannot choose the approver" } } as PeerAssignmentProposal;
    await actOnPeerProposal(proposal, action, "authoring-workshop");
    expect(mocks.unary).toHaveBeenCalledWith(`coordination.channels.by_channel_id.proposals.by_proposal_id.${action}.post`, { channel_id: "channel/one", proposal_id: "immutable-proposal" }, {}, "authoring-workshop");
  });
});
