/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const api = vi.hoisted(() => ({ invoke: vi.fn(), unary: vi.fn(), select: vi.fn(), discover: vi.fn(), reset: vi.fn(), scope: "local" }));
vi.mock("@tauri-apps/api/core", () => ({ invoke:api.invoke }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary:api.unary }));
vi.mock("$lib/window", () => ({ isTauri:() => true }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat:{get workshopScopeId() { return api.scope; }} }));
vi.mock("$lib/code/hostedReviews.svelte", () => ({ hostedReviews:{providers:[{id:"github",label:"GitHub",hosts:["github.com"],operations:["device_auth"]}],connectionId:null,discover:api.discover,reset:api.reset,selectAccount:api.select} }));
import SettingsAccountsSection from "./SettingsAccountsSection.svelte";
let component: ReturnType<typeof mount> | null = null;
const account = { connection_id:"account-1",kind:"github",label:"person",base_url:"https://github.com",secrets:{oauth_bundle:true} };
const challenge = {status:"pending",flow_id:"flow-1",user_code:"ABCD-EFGH",verification_uri:"https://github.com/login/device",interval:5,expires_in:900};
const connected = {status:"connected",connection_id:"account-1",account:"person"};
async function settle() { for (let i = 0; i < 12; i++) { await Promise.resolve(); flushSync(); } }
async function advance(ms: number) { await vi.advanceTimersByTimeAsync(ms); await settle(); }
function button(label: string) { return [...document.querySelectorAll("button")].find((b) => b.textContent?.includes(label))!; }
beforeEach(() => {
  vi.useFakeTimers(); vi.resetAllMocks(); api.scope="local"; api.unary.mockResolvedValue({connections:[]});
  api.invoke.mockImplementation((_command, {request}) => Promise.resolve(request.action === "poll" ? connected : request.action === "cancel" ? {status:"cancelled"} : challenge));
});
afterEach(async () => { if (component) await unmount(component); component=null; document.body.replaceChildren(); vi.useRealTimers(); });
it("shows a device code and opens GitHub without client configuration or a callback", async () => {
  component=mount(SettingsAccountsSection,{target:document.body}); await settle();
  button("Sign in with GitHub").click(); await settle();
  expect(api.invoke).toHaveBeenCalledWith("review_provider_auth",{request:{action:"begin",provider_id:"github",host:"github.com"}});
  expect(api.invoke).toHaveBeenCalledWith("review_provider_auth",{request:{action:"open",flow_id:"flow-1"}});
  expect(document.body.textContent).toContain("ABCD-EFGH");
  expect(document.querySelector("input")).toBeNull();
  expect(document.body.textContent).not.toContain("client ID");
  expect(api.unary.mock.calls.some(([operation]) => operation === "reviews.auth.post")).toBe(false);
});
it("honors poll intervals and slowdown, then selects the completed account automatically", async () => {
  let polls=0;
  api.invoke.mockImplementation((_command, {request}) => Promise.resolve(request.action === "poll" ? (++polls === 1 ? {...challenge, interval:10} : connected) : challenge));
  component=mount(SettingsAccountsSection,{target:document.body}); await settle(); button("Sign in with GitHub").click(); await settle();
  await advance(4999); expect(polls).toBe(0);
  await advance(1); expect(polls).toBe(1);
  await advance(9999); expect(polls).toBe(1);
  api.unary.mockResolvedValue({connections:[account]});
  await advance(1); expect(polls).toBe(2);
  expect(api.select).toHaveBeenCalledWith("account-1","github","github.com");
  expect(document.body.textContent).toContain("Connected as person.");
  expect(document.body.textContent).not.toContain("ABCD-EFGH");
  expect(api.invoke).toHaveBeenCalledWith("review_provider_auth",{request:{action:"cancel",flow_id:"flow-1"}});
});
it("cancels the pinned flow and stops polling", async () => {
  component=mount(SettingsAccountsSection,{target:document.body}); await settle(); button("Sign in with GitHub").click(); await settle();
  button("Cancel").click(); await settle(); await advance(20000);
  expect(api.invoke.mock.calls.filter(([,args]) => args.request.action === "poll")).toHaveLength(0);
  expect(api.invoke).toHaveBeenCalledWith("review_provider_auth",{request:{action:"cancel",flow_id:"flow-1"}});
  expect(button("Sign in with GitHub").disabled).toBe(false);
});
it("pauses on a failed check and retries the same flow without another authorization", async () => {
  let polls=0;
  api.invoke.mockImplementation((_command, {request}) => request.action === "poll" && ++polls === 1 ? Promise.reject('HTTP 400: {"message":"Connection interrupted."}') : Promise.resolve(request.action === "poll" ? connected : challenge));
  component=mount(SettingsAccountsSection,{target:document.body}); await settle(); button("Sign in with GitHub").click(); await settle(); await advance(5000);
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("Connection interrupted.");
  await advance(30000); expect(polls).toBe(1);
  api.unary.mockResolvedValue({connections:[account]});
  button("Retry connection").click(); await settle();
  expect(api.select).toHaveBeenCalledWith("account-1","github","github.com");
  expect(api.invoke.mock.calls.filter(([,args]) => args.request.action === "begin")).toHaveLength(1);
});
it("discards a late challenge after switching workshops, using its native pinned flow", async () => {
  let resolve!: (value: unknown) => void;
  api.invoke.mockImplementation((_command,{request}) => request.action === "begin" ? new Promise((done) => resolve=done) : Promise.resolve({status:"cancelled"}));
  component=mount(SettingsAccountsSection,{target:document.body}); await settle(); button("Sign in with GitHub").click(); await settle();
  api.scope="remote"; resolve(challenge); await settle();
  expect(api.select).not.toHaveBeenCalled();
  expect(api.invoke).toHaveBeenCalledWith("review_provider_auth",{request:{action:"cancel",flow_id:"flow-1"}});
});
it("shows a readable startup failure and allows sign-in again", async () => {
  api.invoke.mockRejectedValue('HTTP 400: {"message":"Sign-in is temporarily unavailable."}');
  component=mount(SettingsAccountsSection,{target:document.body}); await settle(); button("Sign in with GitHub").click(); await settle();
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("Sign-in is temporarily unavailable.");
  expect(button("Sign in with GitHub").disabled).toBe(false);
});
it("disconnects accounts through the existing integration API", async () => {
  api.unary.mockResolvedValue({connections:[account]});
  component=mount(SettingsAccountsSection,{target:document.body}); await settle();
  document.querySelector<HTMLButtonElement>('[aria-label="Disconnect person"]')!.click(); await settle();
  expect(api.unary).toHaveBeenCalledWith("integrations.by_connection_id.delete",{connection_id:"account-1"});
});
