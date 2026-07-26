import { describe, expect, it } from "vitest";
import { parseSpotlightQuery } from "./collectCommands";

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
