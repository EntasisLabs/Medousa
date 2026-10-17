/** @vitest-environment happy-dom */
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const fixture = vi.hoisted(() => ({ scope: "one", session: "other", switchSession: vi.fn(), promote: vi.fn(), focus: vi.fn() }));
vi.mock("$lib/components/chat/ChatMessageList.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/LiquidCardDetailSheet.svelte", () => ({ default: () => {} }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { get sessionId() { return fixture.session; }, messagesFor: (session: string) => [{ id: "ask", role: "user", lane: "ask", askJobId: "job", content: `${session} ask` }], switchSession: fixture.switchSession, promoteAskToChat: fixture.promote } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { isMobile: false } }));
vi.mock("$lib/stores/automationsNav.svelte", () => ({ automationsNav: {} }));
vi.mock("$lib/stores/flowDraft.svelte", () => ({ flowDraft: {} }));
vi.mock("$lib/utils/saveChatTurnToVault", () => ({ saveChatTurnToVault: vi.fn(), showChatTurnSaveFeedback: vi.fn() }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => fixture.scope }));
vi.mock("$lib/runtime/shellUseCases", () => ({ focusChatComposer: fixture.focus }));
vi.mock("$lib/utils/workChromeEvents", () => ({ openNewAsk: vi.fn() }));
import ChatAsksPanel from "./ChatAsksPanel.svelte";
let component: ReturnType<typeof mount> | null = null;
const move = () => [...document.querySelectorAll('button')].find((button) => button.textContent?.trim() === "Move to chat")!;
beforeEach(() => { vi.clearAllMocks(); fixture.scope = "one"; fixture.session = "other"; fixture.switchSession.mockImplementation(async (id: string) => { fixture.session = id; }); });
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); });
describe("conversation ask transcripts", () => {
  it("reads and promotes the originating conversation instead of the active one", async () => {
    component = mount(ChatAsksPanel, { target: document.body, props: { sessionId: "origin" } }); flushSync();
    expect(document.body.textContent).toContain("origin ask");
    move().click(); await new Promise((resolve) => setTimeout(resolve, 0));
    expect(fixture.switchSession).toHaveBeenCalledWith("origin"); expect(fixture.promote).toHaveBeenCalledWith("job"); expect(fixture.focus).toHaveBeenCalledOnce();
  });
  it("does not promote or navigate after a workshop changes while switching conversation", async () => {
    let finish!: () => void;
    fixture.switchSession.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
    component = mount(ChatAsksPanel, { target: document.body, props: { sessionId: "origin" } }); flushSync();
    move().click(); fixture.scope = "two"; finish(); await Promise.resolve();
    expect(fixture.promote).not.toHaveBeenCalled(); expect(fixture.focus).not.toHaveBeenCalled();
  });
});
