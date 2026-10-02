import { expect, it } from "vitest";
import { codeRuntimeIssue } from "./codeRuntimeIssue";
it("distinguishes branch drift from merge conflicts", () => {
  const raw = 'workshop returned HTTP 409 Conflict: {"error":"environment drifted after seal: attached checkout switched branches: expected medousa/old, found medousa/new","kind":"conflict"}';
  const issue = codeRuntimeIssue(raw);
  expect(issue.summary).toBe("The working copy changed branches.");
  expect(issue.guidance).toContain("medousa/old");
  expect(issue.guidance).toContain("medousa/new");
  expect(issue.details).toBe(raw);
});
it("keeps unknown transport payloads in Details", () => {
  expect(codeRuntimeIssue('workshop returned HTTP 500: {"internal_id":"id"}').summary).toBe("The workshop could not complete this action.");
});
