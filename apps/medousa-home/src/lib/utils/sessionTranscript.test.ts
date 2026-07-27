import { describe, expect, it } from "vitest";
import {
  sessionExportBasename,
  sessionTranscriptMarkdown,
} from "./sessionTranscript";

describe("sessionTranscriptMarkdown", () => {
  it("formats user and assistant turns", () => {
    const md = sessionTranscriptMarkdown({
      session_id: "sess-abcdefgh",
      turns: [
        {
          role: "user",
          content: "Hello",
          timestamp: "2026-07-25T00:00:00Z",
        },
        {
          role: "assistant",
          content: "Hi there",
          timestamp: "2026-07-25T00:00:01Z",
        },
      ],
    });
    expect(md).toContain("# Conversation sess-abc");
    expect(md).toContain("## You");
    expect(md).toContain("Hello");
    expect(md).toContain("## Medousa");
    expect(md).toContain("Hi there");
  });

  it("uses custom title and basename", () => {
    expect(sessionExportBasename("abc12345xyz")).toBe("medousa-session-abc12345");
    const md = sessionTranscriptMarkdown(
      { session_id: "x", turns: [] },
      { title: "Project chat" },
    );
    expect(md).toContain("# Project chat");
    expect(md).toContain("No messages");
  });
});
