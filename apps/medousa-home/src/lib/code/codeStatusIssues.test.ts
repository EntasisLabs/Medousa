import { expect, it } from "vitest";
import { codeStatusIssues } from "./codeStatusIssues";

it("presents the same branch drift once across editor errors and the working terminal", () => {
  const message = 'HTTP 409: {"error":"attached checkout switched branches: expected old, found new"}';
  const issues = codeStatusIssues({
    messages: [message, message],
    terminalContext: { cwd: "/repo", current_branch: "new", attached_branch: "old" },
    analysisError: "HTTP 500", analysisLoaded: false, unavailableLanguages: [],
  });
  expect(issues.map((issue) => issue.label)).toEqual(["Working copy changed", "Analysis unavailable"]);
  expect(issues[0].details).toBe(message);
  expect(issues[1].guidance).not.toContain("connection");
});

it("keeps unavailable analysis distinct from stale successful observations", () => {
  expect(codeStatusIssues({ messages: [], analysisError: "failed", analysisLoaded: true, unavailableLanguages: [] })[0].label).toBe("Results stale");
  expect(codeStatusIssues({ messages: [], analysisError: null, analysisLoaded: true, unavailableLanguages: ["rust"] })[0].label).toBe("Analysis incomplete");
});
