import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("native Live personal-context prompt contract", () => {
  it("includes concrete recall examples and excludes casual storytelling", () => {
    const source = readFileSync(new URL("../../src-tauri/src/live_voice.rs", import.meta.url), "utf8");
    expect(source).toContain("what I've been up to this week");
    expect(source).toContain("what did we decide last time?");
    expect(source).toContain("small seeded history is not a complete activity log");
    expect(source).toContain("user is simply telling you about their week");
    expect(source).toContain("voice_instructions.push_str(PERSONAL_CONTEXT_HANDOFF)");
    expect(source).toContain("instructions.push_str(PERSONAL_CONTEXT_HANDOFF)");
  });
});
