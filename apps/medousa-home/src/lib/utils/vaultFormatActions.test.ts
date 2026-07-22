import { describe, expect, it } from "vitest";
import {
  liveBlockStyleFromActions,
  VAULT_LIVE_BLOCK_STYLE_OPTIONS,
} from "./vaultFormatActions";

describe("liveBlockStyleFromActions", () => {
  it("defaults to paragraph", () => {
    expect(liveBlockStyleFromActions([]).short).toBe("P");
  });

  it("surfaces the active heading", () => {
    expect(liveBlockStyleFromActions(["bold", "h2"]).short).toBe("H2");
    expect(liveBlockStyleFromActions(["h1"]).label).toBe("Title");
  });

  it("prefers heading over paragraph when both appear", () => {
    const opt = liveBlockStyleFromActions(["paragraph", "h3"]);
    expect(opt.action).toBe("h3");
  });

  it("lists all block style options", () => {
    expect(VAULT_LIVE_BLOCK_STYLE_OPTIONS.map((o) => o.short)).toEqual([
      "P",
      "H1",
      "H2",
      "H3",
    ]);
  });
});
