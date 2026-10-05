/** @vitest-environment happy-dom */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BotProfile } from "$lib/types/generated/daemon_api";
const state = vi.hoisted(() => ({scope:"mac",epoch:1,session:"ada"}));
const api = vi.hoisted(() => ({call:vi.fn(),reload:vi.fn().mockResolvedValue(undefined)}));
vi.mock("$lib/daemon/contractClient", () => ({daemonUnary:api.call}));
vi.mock("$lib/stores/chat.svelte", () => ({chat:{get workshopScopeId(){return state.scope;},get workshopEpoch(){return state.epoch;},get focusedSessionId(){return state.session;},reloadCurrentSession:api.reload}}));
import { admitRuntimeBotTurn, cancelRuntimeBotTurn, loadRuntimeBotJobs, refreshRuntimeBotTurn, runtimeBotJob, runtimeBotJobPending, sendRuntimeBotTurn } from "./runtimeBotTurns.svelte";
const bot = {bot_id:"bot-ada",external_agent:{runtime:"cursor"}} as unknown as BotProfile;
beforeEach(() => {vi.clearAllMocks();localStorage.clear();state.scope=`mac-${++state.epoch}`;state.session="ada";loadRuntimeBotJobs();});
describe("durable runtime Bot requests", () => {
  it("retries uncertain admission with the exact request identity and conversation", async () => {
    api.call.mockRejectedValueOnce(new Error("lost response")).mockResolvedValueOnce({ticket:{jobId:"job",turnId:"turn"}});
    await expect(sendRuntimeBotTurn(bot,"ada","fix tests")).rejects.toThrow("lost response");
    const original = api.call.mock.calls[0][2];await admitRuntimeBotTurn("ada");
    expect(api.call.mock.calls[1][2]).toEqual(original);expect(original).toMatchObject({bot:"bot-ada",session_id:"ada",prompt:"fix tests"});
    expect(runtimeBotJob("ada")?.jobId).toBe("job");expect(api.reload).toHaveBeenCalledOnce();
  });
  it("recovers an interrupted submission with its original retry ID", async () => {
    const originalScope = state.scope;
    api.call.mockReturnValueOnce(new Promise(() => {}));void sendRuntimeBotTurn(bot,"ada","work");
    const requestId = runtimeBotJob("ada")!.requestId;
    state.scope="other";loadRuntimeBotJobs();state.scope=originalScope;loadRuntimeBotJobs();
    expect(runtimeBotJob("ada")).toMatchObject({status:"submission_uncertain",requestId});
    api.call.mockResolvedValueOnce({ticket:{jobId:"recovered",turnId:"turn"}});await admitRuntimeBotTurn("ada");
    expect(api.call.mock.calls[1][2].request_id).toBe(requestId);
  });
  it("deduplicates simultaneous retries and retains completion if transcript reload fails", async () => {
    api.call.mockRejectedValueOnce(new Error("lost response"));
    await expect(sendRuntimeBotTurn(bot,"ada","work")).rejects.toThrow();
    let finish!: (value:unknown) => void;
    api.call.mockReturnValueOnce(new Promise((resolve) => finish=resolve));
    const first = admitRuntimeBotTurn("ada");
    const second = admitRuntimeBotTurn("ada");
    expect(api.call).toHaveBeenCalledTimes(2);
    finish({ticket:{jobId:"recovered",turnId:"turn"}});
    await Promise.all([first,second]);
    api.call.mockResolvedValueOnce({status:"completed"});
    api.reload.mockRejectedValueOnce(new Error("history offline"));
    await refreshRuntimeBotTurn("ada");
    expect(runtimeBotJob("ada")?.status).toBe("completed");
    expect(runtimeBotJobPending("ada")).toBe(false);
  });
  it("does not import admission into a different workshop", async () => {
    let finish!: (value:unknown) => void;api.call.mockReturnValue(new Promise((resolve)=>finish=resolve));
    const pending = sendRuntimeBotTurn(bot,"ada","work");state.scope="mini";state.epoch++;loadRuntimeBotJobs();
    finish({ticket:{jobId:"old",turnId:"old"}});await pending;expect(runtimeBotJob("ada")).toBeNull();expect(api.reload).not.toHaveBeenCalled();
  });
  it("blocks duplicate sends, observes completion, and cancels the exact job", async () => {
    api.call.mockResolvedValueOnce({ticket:{jobId:"job",turnId:"turn"}});await sendRuntimeBotTurn(bot,"ada","work");
    await expect(sendRuntimeBotTurn(bot,"ada","another")).rejects.toThrow("already");
    api.call.mockResolvedValueOnce({}).mockResolvedValueOnce({status:"cancelled"});await cancelRuntimeBotTurn("ada");
    expect(api.call).toHaveBeenCalledWith("bots.ask.by_job_id.cancel.post",{job_id:"job"});expect(runtimeBotJobPending("ada")).toBe(false);
  });
});
