/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
vi.mock("$lib/platform", async (original) => ({ ...await original<typeof import("$lib/platform")>(), isTauriDesktop: () => true }));
import { parseSpotlightQuery, collectWorkshopLensCommands, collectWorkshopCommands, flattenGroups } from "./collectCommands";
import { buildWorkshopCommandContext } from "./context";

describe("parseSpotlightQuery", () => {
  it("strips create / run / advanced prefixes", () => {
    expect(parseSpotlightQuery("+note")).toEqual({
      mode: "create",
      rawQuery: "note",
      input: "+note",
    });
    expect(parseSpotlightQuery("!hello")).toEqual({
      mode: "run",
      rawQuery: "hello",
      input: "!hello",
    });
    expect(parseSpotlightQuery(">export")).toEqual({
      mode: "advanced",
      rawQuery: "export",
      input: ">export",
    });
    expect(parseSpotlightQuery("library")).toEqual({
      mode: "default",
      rawQuery: "library",
      input: "library",
    });
  });
});

it("includes both terminal actions in Actions and saved SSH servers in global search", () => {
  const ctx = buildWorkshopCommandContext({ close: vi.fn(), focusChat: vi.fn() });
  const actions = flattenGroups(collectWorkshopLensCommands(ctx, "actions"));
  expect(actions.filter((command) => command.id.startsWith("terminal-")).map((command) => command.id)).toEqual(["terminal-open", "terminal-open-ssh"]);
  const hits = flattenGroups(collectWorkshopCommands(ctx, { query: "prodesk", workshopScope: "mac", sshTargets: [{ target_id: "ssh-prodesk", name: "ProDesk", host: "10.12.0.16", port: 22, username: "ops", agent_access: true }] }));
  expect(hits[0].id).toBe("terminal-ssh:ssh-prodesk");
  expect(flattenGroups(collectWorkshopCommands(ctx, { query: "Open terminal" }))[0].id).toBe("terminal-open");
});
