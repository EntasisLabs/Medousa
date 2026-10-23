import { describe, expect, it } from "vitest";
import {
  friendlyMediaUploadError,
  friendlyTurnError,
  friendlyUserError,
} from "./normieErrors";

describe("friendlyMediaUploadError", () => {
  it("maps oversized files", () => {
    expect(
      friendlyMediaUploadError("file exceeds max size (26214400 bytes)", "paper.pdf"),
    ).toContain("25 MB");
    expect(friendlyMediaUploadError("file exceeds max size (26214400 bytes)", "paper.pdf")).toContain(
      "paper.pdf",
    );
  });

  it("maps disallowed mime types", () => {
    expect(friendlyMediaUploadError("mime type not allowed: application/octet-stream")).toContain(
      "isn't supported",
    );
  });
});

describe("friendlyTurnError", () => {
  it("hides provider status codes", () => {
    expect(friendlyTurnError("openai HTTP 429: rate limit exceeded")).not.toContain("429");
    expect(friendlyTurnError("openai HTTP 429: rate limit exceeded")).toContain("rate-limiting");
  });

  it("passes through short operator copy", () => {
    expect(friendlyTurnError("Turn cancelled.")).toBe("Turn cancelled.");
  });
});

describe("friendlyUserError", () => {
  it("prefers media mapping", () => {
    expect(friendlyUserError("empty file", { fileName: "x.pdf" })).toContain("empty");
  });
});

it("does not turn missing models, sessions, or routes into missing files", () => {
  expect(friendlyUserError("model not found")).toContain("model isn't available");
  expect(friendlyUserError("session not found")).toContain("conversation isn't available");
  expect(friendlyUserError("route not found")).not.toContain("file");
  expect(friendlyUserError("attachment not found", { fileName: "photo.png" })).toContain("Couldn't read that file");
  expect(friendlyUserError("port failure: ChatGPT sign-in must be completed again")).toContain("Reconnect ChatGPT");
});
