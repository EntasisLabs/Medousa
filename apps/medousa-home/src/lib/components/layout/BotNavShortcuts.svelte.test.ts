/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import BotNavShortcuts from "./BotNavShortcuts.svelte";

const port = vi.hoisted(() => ({
  scope: "one",
  bot: { bot_id: "bot-one", display_name: "One", archived: false, avatar_ref: null },
  spec: { profileId: "personal", nav: { botPins: ["bot-one"], botAccents: {} } } as any,
  open: vi.fn(), refresh: vi.fn(), refreshSessions: vi.fn(), openChat: vi.fn(), switchSession: vi.fn(),
  get: vi.fn(), put: vi.fn(), apply: vi.fn(),
}));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { bots: [port.bot], open: port.open, refresh: port.refresh, error: null } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { refreshSessions: port.refreshSessions, switchSession: port.switchSession } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openChat: port.openChat } }));
vi.mock("$lib/stores/environment.svelte", () => ({ environment: { get spec() { return port.spec; }, applySpec: port.apply } }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => port.scope }));
vi.mock("$lib/daemon/environment", () => ({ getEnvironmentSpec: port.get, putEnvironmentSpec: port.put }));
let component: ReturnType<typeof mount> | undefined;
const select = vi.fn();
beforeEach(() => {
  vi.clearAllMocks(); port.scope = "one";
  port.spec = { profileId: "personal", nav: { botPins: ["bot-one"], botAccents: {} } };
  port.refresh.mockResolvedValue(undefined);
  port.open.mockResolvedValue({ bot: port.bot, binding: { session_id: "canonical" } });
  port.refreshSessions.mockResolvedValue(undefined); port.openChat.mockReturnValue("tab");
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });
function render(editing = false) { component = mount(BotNavShortcuts, { target: document.body, props: { editing, onSelect: select } }); flushSync(); }
it("opens the canonical Bot session through shell tabs, never a new chat", async () => {
  render(); document.querySelector<HTMLButtonElement>('button[title="One"]')!.click();
  await vi.waitFor(() => expect(select).toHaveBeenCalledWith("chat"));
  expect(port.openChat).toHaveBeenCalledWith("canonical", { activate: true, title: "One" });
  expect(port.switchSession).not.toHaveBeenCalled();
});
it("discards an open result after switching workshop", async () => {
  let resolve!: (value: unknown) => void;
  port.open.mockReturnValue(new Promise((done) => { resolve = done; }));
  render(); document.querySelector<HTMLButtonElement>('button[title="One"]')!.click();
  port.scope = "two"; resolve({ bot: port.bot, binding: { session_id: "wrong-authority" } });
  await Promise.resolve(); await Promise.resolve();
  expect(port.openChat).not.toHaveBeenCalled(); expect(select).not.toHaveBeenCalled();
});
it("unpins with a fresh daemon spec and preserves native-only metadata", async () => {
  port.get.mockResolvedValue({ spec: { ...port.spec, nav: { ...port.spec.nav, nativeOrder: ["terminal", "chat"] } }, revision: 3 });
  port.put.mockImplementation(async (request) => ({ spec: request.spec, revision: 4 }));
  render(true); document.querySelector<HTMLButtonElement>('button[aria-label="Unpin One"]')!.click();
  await vi.waitFor(() => expect(port.apply).toHaveBeenCalled());
  expect(port.put.mock.calls[0][0].spec.nav).toEqual({ botPins: [], botAccents: {}, nativeOrder: ["terminal", "chat"] });
});
