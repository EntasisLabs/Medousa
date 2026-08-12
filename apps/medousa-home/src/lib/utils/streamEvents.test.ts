import { describe, expect, it } from "vitest";

import { isRecoverableStreamError } from "$lib/utils/streamEvents";

describe("isRecoverableStreamError", () => {
  it.each([
    "read HTTP response",
    "read iroh HTTP body: connection lost: timed out",
    "write SSE chunk: connection lost: closed by peer: 0",
    "SSE stream ended unexpectedly",
  ])("treats transient remote stream failure as recoverable: %s", (message) => {
    expect(isRecoverableStreamError(message)).toBe(true);
  });

  it("does not classify malformed stream payloads as recoverable", () => {
    expect(isRecoverableStreamError("invalid SSE JSON: expected value")).toBe(false);
  });
});
