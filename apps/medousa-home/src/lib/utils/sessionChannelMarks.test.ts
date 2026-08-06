import { describe, expect, it } from "vitest";
import {
  normalizeSessionChannelSurface,
  resolveSessionChannelMarks,
  sessionChannelTitle,
} from "./sessionChannelMarks";

describe("sessionChannelMarks", () => {
  it("normalizes known surfaces and rejects home/notion", () => {
    expect(normalizeSessionChannelSurface("VSCode")).toBe("vscode");
    expect(normalizeSessionChannelSurface("home")).toBeNull();
    expect(normalizeSessionChannelSurface("notion")).toBeNull();
  });

  it("resolves catalog fields for the rail", () => {
    expect(
      resolveSessionChannelMarks({
        origin_surface: "neovim",
        has_code_work: true,
      }),
    ).toEqual({ channel: "neovim", hasCodeWork: true });
    expect(resolveSessionChannelMarks({})).toEqual({
      channel: null,
      hasCodeWork: false,
    });
  });

  it("labels channel marks", () => {
    expect(sessionChannelTitle("browser")).toBe("Browser");
  });
});
