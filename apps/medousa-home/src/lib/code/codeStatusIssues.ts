import { codeRuntimeIssue } from "./codeRuntimeIssue";
import type { TerminalSessionSummary } from "$lib/terminal";

export type CodeStatusIssue = {
  id: string;
  label: string;
  summary: string;
  guidance: string;
  details: string;
};

/** Presentation only: observations never grant editing authority or adopt a checkout. */
export function codeStatusIssues(input: {
  messages: Array<string | null | undefined>;
  terminalContext?: TerminalSessionSummary["workspace_context"];
  analysisError: string | null;
  analysisLoaded: boolean;
  unavailableLanguages: string[];
}): CodeStatusIssue[] {
  const issues: CodeStatusIssue[] = [];
  for (const message of new Set(input.messages.filter((value): value is string => Boolean(value)))) {
    const issue = codeRuntimeIssue(message);
    const workingCopy = /working copy changed/i.test(issue.summary);
    const id = workingCopy ? "working-copy" : `operation:${message}`;
    if (issues.some((entry) => entry.id === id)) continue;
    issues.push({ ...issue, id, label: workingCopy ? "Working copy changed" : "Action failed" });
  }
  const context = input.terminalContext;
  if (context?.attached_branch && context.attached_branch !== context.current_branch &&
      !issues.some((issue) => issue.id === "working-copy")) {
    issues.unshift({
      id: "working-copy", label: "Working copy changed", summary: "The working copy changed branches.",
      guidance: `Now on ${context.current_branch ?? "detached HEAD"}. This project was attached to ${context.attached_branch}. Terminal uses the current working folder; tracked editing still requires a matching project checkout.`,
      details: "",
    });
  }
  if (input.analysisError || input.unavailableLanguages.length) {
    const label = input.analysisError
      ? input.analysisLoaded ? "Results stale" : "Analysis unavailable"
      : "Analysis incomplete";
    issues.push({
      id: "analysis", label, summary: label,
      guidance: input.analysisError
        ? "Project analysis could not be refreshed. Any existing observations remain visible. Retry from Problems."
        : `Results are incomplete for ${input.unavailableLanguages.join(", ")}.`,
      details: input.analysisError ?? "",
    });
  }
  return issues;
}
