import { describe, expect, it, vi } from "vitest";
import { desktopWindowCommands } from "./commands";
import type { WorkshopCommandContext } from "$lib/commands/types";
import type { ItemProjection } from "$lib/forge";

describe("desktop destinations", () => {
  it("opens concrete resources with workshop ownership and dismisses only after success", async () => {
    const open = vi.fn().mockResolvedValue("window"), close = vi.fn();
    const commands = desktopWindowCommands("remote-workshop", [], [{ id: "canvas", title: "Design", pinned: false, revision: 1, updated_at: "now" }], [{ id: "project", title: "App" } as ItemProjection], open, vi.fn(), vi.fn());
    const ctx = { callbacks: { close } } as unknown as WorkshopCommandContext;
    await commands.find(c => c.id === "desktop-create:canvas")!.run(ctx);
    expect(open).toHaveBeenCalledWith({ kind: "create", workshopId: "remote-workshop", resourceId: "canvas", title: "Design" });
    expect(close).toHaveBeenCalledOnce();
    open.mockRejectedValueOnce(new Error("Couldn’t create window"));
    await expect(commands.find(c => c.id === "desktop-code:project")!.run(ctx)).rejects.toThrow("Couldn’t create window");
    expect(close).toHaveBeenCalledOnce();
  });
  it("keeps browse windows distinct from conversations and creates chats explicitly", async () => {
    const open = vi.fn(), newChat = vi.fn(), close = vi.fn();
    const commands = desktopWindowCommands("personal", [{ session_id: "session", display_name: "Hello", preview: "Hi", turns: 1, verification_runs: 0 }], [], [], open, newChat, vi.fn());
    const ctx = { callbacks: { close } } as unknown as WorkshopCommandContext;
    await commands.find(c => c.id === "desktop-chat:session")!.run(ctx);
    expect(open).toHaveBeenLastCalledWith({ kind: "chat", workshopId: "personal", resourceId: "session", title: "Hello" });
    await commands.find(c => c.id === "desktop-chat")!.run(ctx);
    expect(open).toHaveBeenLastCalledWith({ kind: "chat", workshopId: "personal" });
    await commands.find(c => c.id === "desktop-new-chat")!.run(ctx);
    expect(newChat).toHaveBeenCalledOnce();
  });
  it("offers existing live PTYs as native destinations without reviving exited sessions", async () => {
    const open = vi.fn();
    const terminal = { session_id: "pty", cwd: "/repo", root_kind: "workspace", work_id: "work" };
    const commands = desktopWindowCommands("personal", [], [], [], open, vi.fn(), vi.fn(), [terminal, { ...terminal, session_id: "dead", exited: true }]);
    expect(commands.some(command => command.id === "desktop-terminal:dead")).toBe(false);
    await commands.find(command => command.id === "desktop-terminal:pty")!.run({ callbacks: { close: vi.fn() } } as unknown as WorkshopCommandContext);
    expect(open).toHaveBeenCalledWith({ kind: "terminal", workshopId: "personal", resourceId: "pty" });
  });
});
