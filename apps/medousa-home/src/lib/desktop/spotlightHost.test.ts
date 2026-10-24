/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
const api = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: api.invoke }));
vi.mock("./windows", () => ({ openDesktopWindow: api.open }));
import { createDesktopSpotlightHost } from "./spotlightHost";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
import type { SpotlightCommandDescriptor } from "$lib/spotlight/host";
const command: SpotlightCommandDescriptor = { id: "open-note:hello", label: "Hello", section: "open", token: "ticket" };
beforeEach(() => { vi.clearAllMocks(); commandSpotlight.openSpotlight(); api.invoke.mockResolvedValue(null); api.open.mockResolvedValue("window"); });

it("pins requests to its original workshop and closes only after acknowledgement", async () => {
  let finish!: () => void;
  api.invoke.mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
  const host = createDesktopSpotlightHost("remote-a");
  const pending = host.execute(command, "new title");
  expect(commandSpotlight.open).toBe(true);
  expect(api.invoke).toHaveBeenCalledWith("desktop_spotlight_request", { workshopId: "remote-a", request: { kind: "execute", token: "ticket", args: "new title" } });
  finish(); await pending;
  expect(commandSpotlight.open).toBe(false);
});
it("keeps search open when the owner cannot execute a command", async () => {
  api.invoke.mockRejectedValueOnce(new Error("Workshop changed"));
  await expect(createDesktopSpotlightHost("a").execute(command)).rejects.toThrow("Workshop changed");
  expect(commandSpotlight.open).toBe(true);
  expect(api.invoke).toHaveBeenLastCalledWith("desktop_spotlight_restore", { workshopId: "a" });
});
it("never resurrects an explicitly dismissed or replaced launcher after a late failure", async () => {
  api.invoke.mockRejectedValueOnce(new Error("Late failure"));
  const host = createDesktopSpotlightHost("a", { isCurrent: () => false });
  await expect(host.execute(command)).rejects.toThrow("Late failure");
  expect(api.invoke).toHaveBeenCalledOnce();
  api.invoke.mockRejectedValueOnce(new Error("Dismissed")); commandSpotlight.closeSpotlight();
  await expect(createDesktopSpotlightHost("a").execute(command)).rejects.toThrow("Dismissed");
  expect(api.invoke).toHaveBeenCalledTimes(2);
});
it("does not close a new opening when an old action finishes", async () => {
  const pending = vi.fn();
  await createDesktopSpotlightHost("a", { isCurrent: () => false, pending }).execute(command);
  expect(commandSpotlight.open).toBe(true);
  expect(pending.mock.calls).toEqual([[1], [-1]]);
});
it("keeps Activity, SSH, command mode and query resume inside the widget", async () => {
  const host = createDesktopSpotlightHost("a");
  for (const id of ["workshop-activity", "terminal-open-ssh", "workbench.action.showCommands"]) {
    await host.execute({ ...command, id });
    expect(commandSpotlight.open).toBe(true);
  }
  expect(commandSpotlight.seedQuery).toBe(">");
  commandSpotlight.rememberQuery("prior search", "notes");
  await host.execute({ ...command, id: "do-resume-spotlight" });
  expect(commandSpotlight.seedQuery).toBe("prior search");
  expect(commandSpotlight.mode).toBe("notes");
  expect(api.invoke).not.toHaveBeenCalled();
});
it("opens concrete Activity chats/projects as native windows and delegates task details", async () => {
  const host = createDesktopSpotlightHost("a");
  await host.openActivity({ kind: "chat", sessionId: "session" }, "Conversation");
  expect(api.open).toHaveBeenLastCalledWith({ kind: "chat", workshopId: "a", resourceId: "session", title: "Conversation" });
  await host.openActivity({ kind: "project", workId: "project" }, "App");
  expect(api.open).toHaveBeenLastCalledWith({ kind: "code", workshopId: "a", resourceId: "project", title: "App" });
  await host.openActivity({ kind: "card", cardId: "task" }, "Build");
  expect(api.invoke).toHaveBeenLastCalledWith("desktop_spotlight_request", { workshopId: "a", request: { kind: "activity", target: { kind: "card", cardId: "task" }, title: "Build" } });
});
it("opens SSH directly as a native terminal while setup retains its shell destination", async () => {
  const host = createDesktopSpotlightHost("a");
  await host.openSsh("nas");
  expect(api.open).toHaveBeenLastCalledWith({ kind: "terminal", workshopId: "a", sshTargetId: "nas" });
  expect(api.invoke).not.toHaveBeenCalled();
  await host.manageConnection();
  expect(api.invoke).toHaveBeenLastCalledWith("desktop_spotlight_request", { workshopId: "a", request: { kind: "connection" } });
});
it("routes both terminal commands directly without asking the main renderer to execute", async () => {
  const host = createDesktopSpotlightHost("a");
  await host.execute({ ...command, id: "terminal-open" });
  expect(api.open).toHaveBeenLastCalledWith({ kind: "terminal", workshopId: "a" });
  await host.execute({ ...command, id: "terminal-ssh:nas" });
  expect(api.open).toHaveBeenLastCalledWith({ kind: "terminal", workshopId: "a", sshTargetId: "nas" });
  expect(api.invoke).not.toHaveBeenCalled();
});
