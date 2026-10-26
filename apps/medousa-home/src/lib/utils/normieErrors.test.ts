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
  it.each([
    ["cannot acquire Coder authority: invalid transition: work work-test is accepted, cannot begin attempt", "already accepted"],
    ["invalid transition: work work-test is awaiting_review, cannot begin attempt", "awaiting review"],
    ["invalid transition: work work-test is discarded, cannot begin attempt", "discarded"],
    ["invalid transition: work work-test is failed, cannot begin attempt", "current state"],
    ["No space left on device (os error 28)", "disk is full"],
    ["disk quota exceeded", "disk is full"],
    ["cannot acquire Coder authority: lease expired", "permission to execute"],
    ["Coder Forge lease rejected: store unavailable", "permission to execute"],
  ])("explains runtime blockers without exposing raw details: %s", (raw, expected) => {
    const result = friendlyTurnError(`${raw}; secret=do-not-surface HTTP 401`);
    expect(result).toContain(expected);
    expect(result).not.toContain("do-not-surface");
    expect(result).not.toContain("API key");
  });

  it.each([
    "execution overloaded: forge command queue is full",
    "HTTP 503: execution overloaded: repository-lane registry is full",
    "execution overloaded: forge queued-byte budget is exhausted (limit 8388608 bytes, this admission needs 404 bytes)",
  ])("identifies local work saturation before provider errors: %s", (raw) => {
    expect(friendlyTurnError(raw)).toBe(
      "Medousa's work queue is full. Wait for active work to finish, then try again.",
    );
  });

  it("explains an operation limit without suggesting a provider retry", () => {
    expect(friendlyTurnError(
      "execution overloaded: estimated retained bytes (1048577) exceed StoreIo class budget (1048576)",
    )).toContain("per-operation data limit");
  });

  it.each([
    "capture failed after 140400 bytes",
    "worker 401 exited",
    "snapshot 429000 could not be saved",
    "workspace does not exist",
    "HTTP 4040 is not a valid status",
  ])("does not mistake runtime data for provider status: %s", (raw) => {
    expect(friendlyTurnError(raw)).not.toMatch(/API key|model isn't available|rate-limiting/);
  });

  it.each(["HTTP 401: denied", "http error: 403 Forbidden", "status code: 401", "status=403", "StatusCode(401)"])("recognizes explicit status wrappers: %s", (raw) => {
    expect(friendlyTurnError(raw)).toContain("API key");
  });

  it("explains workspace custody and unknown errors without blind retry advice", () => {
    expect(friendlyTurnError("cannot acquire Coder authority: workspace busy: the undertaking workspace is held by an executor outside Coder shared-space coordination")).toContain("another executor");
    expect(friendlyTurnError("")).toContain("error details");
    expect(friendlyTurnError("tool=unknown failure")).not.toContain("Try again in a moment");
    expect(friendlyTurnError("execution timed out")).not.toContain("shorter message");
  });

  it("hides provider status codes", () => {
    expect(friendlyTurnError("openai HTTP 429: rate limit exceeded")).not.toContain("429");
    expect(friendlyTurnError("openai HTTP 429: rate limit exceeded")).toContain("rate-limiting");
  });

  it("passes through short operator copy", () => {
    expect(friendlyTurnError("Turn cancelled.")).toBe("Turn cancelled.");
  });
});

describe("friendlyUserError", () => {
  it.each([
    ["cannot acquire Coder authority: permission denied", "permission to execute"],
    ["Coder Forge lease rejected: no such file", "permission to execute"],
    ["store failed to read file: No space left on device", "disk is full"],
    ["snapshot permission denied: disk quota exceeded", "disk is full"],
    ["invalid transition: work work-test is accepted, cannot begin attempt; permission denied", "already accepted"],
  ])("does not mislabel runtime failures as attachment errors: %s", (raw, expected) => {
    expect(friendlyUserError(raw, { fileName: "attached.png" })).toContain(expected);
  });
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
