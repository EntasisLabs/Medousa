/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const api = vi.hoisted(() => ({ sessions: vi.fn(), open: vi.fn(), main: vi.fn(), pane: null as Record<string, unknown> | null }));
vi.mock("$lib/terminal", () => ({ terminalSessions: api.sessions }));
vi.mock("$lib/desktop/windows", () => ({ openDesktopWindow: api.open }));
vi.mock("$lib/window", () => ({ showMainWindow: api.main }));
vi.mock("./TerminalPane.svelte", () => ({ default: (_anchor: unknown, props: Record<string, unknown>) => { api.pane = props; } }));
import DesktopTerminal from "./DesktopTerminal.svelte";
let component: ReturnType<typeof mount> | undefined;
const metadata = { session_id: "pty", work_id: "project", cwd: "/repo", root_kind: "workspace" };
beforeEach(() => { vi.resetAllMocks(); api.pane = null; api.sessions.mockResolvedValue([metadata]); api.open.mockResolvedValue("window"); api.main.mockResolvedValue(undefined); });
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });
async function start() {
  const close = vi.fn();
  component = mount(DesktopTerminal, { target: document.body, props: { sessionId: "pty", workshopId: "remote-a", title: "Shell", onClose: close } });
  await vi.waitFor(() => { flushSync(); expect(api.pane).not.toBeNull(); });
  return close;
}
it("reuses the PTY pane with pinned workshop transport and native session/file destinations", async () => {
  const close = await start();
  expect(api.sessions).toHaveBeenCalledWith(null);
  expect(api.pane).toMatchObject({ sessionId: "pty", executionRuntimeId: null, workId: "project", worktreeRoot: "/repo" });
  await (api.pane!.onOpenSession as (id: string) => Promise<unknown>)("another-pty");
  expect(api.open).toHaveBeenCalledWith({ kind: "terminal", workshopId: "remote-a", resourceId: "another-pty" });
  (api.pane!.onOpenPath as (path: string) => void)("src/main.ts");
  expect(api.open).toHaveBeenLastCalledWith({ kind: "code", workshopId: "remote-a", resourceId: "project", path: "src/main.ts" });
  (api.pane!.onClose as () => void)(); expect(close).toHaveBeenCalledOnce();
  expect(api.main).not.toHaveBeenCalled();
});
it("only opens the main window for an explicit host-management action", async () => {
  await start();
  (api.pane!.onManageHost as () => void)();
  expect(api.main).toHaveBeenCalledOnce();
});
it("does not mount a missing or exited session as a fresh shell", async () => {
  api.sessions.mockResolvedValue([{ ...metadata, exited: true }]);
  component = mount(DesktopTerminal, { target: document.body, props: { sessionId: "pty", workshopId: "remote-a", title: "Shell", onClose: vi.fn() } });
  await vi.waitFor(() => { flushSync(); expect(document.body.textContent).toContain("session has ended"); });
  expect(api.pane).toBeNull(); expect(api.open).not.toHaveBeenCalled();
});
