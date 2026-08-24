import { describe, expect, it } from "vitest";
import {
  sessionExportBasename,
  sessionTranscriptMarkdown,
} from "./sessionTranscript";

describe("sessionTranscriptMarkdown", () => {
  it("formats user and assistant turns", () => {
    const md = sessionTranscriptMarkdown({
      authority_id: "auth_test",
      session_id: "sess-abcdefgh",
      turns: [
        {
          entry_id: "entry_user",
          entry_seq: 1,
          content_digest: "digest_user",
          role: "user",
          content: "Hello",
          timestamp: "2026-07-25T00:00:00Z",
          tool_names: [],
        },
        {
          entry_id: "entry_assistant",
          entry_seq: 2,
          content_digest: "digest_assistant",
          role: "assistant",
          content: "Hi there",
          timestamp: "2026-07-25T00:00:01Z",
          tool_names: [],
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
      { authority_id: "auth_test", session_id: "x", turns: [] },
      { title: "Project chat" },
    );
    expect(md).toContain("# Project chat");
    expect(md).toContain("No messages");
  });
});
