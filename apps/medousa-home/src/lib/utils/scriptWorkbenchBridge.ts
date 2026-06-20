/** Scripts workbench ↔ scoped chat bridge. */

import { parseGraphemeRunResult } from "$lib/grapheme/graphemeRunOutput";
import type { GraphemeRunResponse } from "$lib/types/grapheme";

export interface ScriptWorkbenchContextScope {
  tabId: string;
  scriptId: string | null;
  name: string;
  lineCount: number;
  dirty: boolean;
}

export function buildScriptWorkbenchContextScope(input: {
  tabId: string;
  scriptId: string | null;
  name: string;
  body: string;
  dirty: boolean;
}): ScriptWorkbenchContextScope {
  return {
    tabId: input.tabId,
    scriptId: input.scriptId,
    name: input.name.trim() || "Untitled script",
    lineCount: input.body.split("\n").length,
    dirty: input.dirty,
  };
}

export function scriptWorkbenchContextHint(scope: ScriptWorkbenchContextScope): string {
  const parts = [`${scope.lineCount} lines`];
  if (scope.scriptId) parts.push(scope.scriptId);
  if (scope.dirty) parts.unshift("Unsaved");
  return parts.join(" · ");
}

function excerptBody(body: string, maxChars = 2400): string {
  const trimmed = body.trim();
  if (trimmed.length <= maxChars) return trimmed;
  return `${trimmed.slice(0, maxChars).trimEnd()}…`;
}

export function buildScriptWorkbenchChatDraft(input: {
  scope: ScriptWorkbenchContextScope;
  body: string;
  runError?: string | null;
  runResult?: GraphemeRunResponse["result"] | null;
  compileError?: string | null;
  compileHints?: string[];
}): string {
  const { scope, body } = input;
  const label = scope.name;
  const idLine = scope.scriptId ? `\`script_id\`: \`${scope.scriptId}\`` : "Unsaved tab (not in library yet)";
  const sections = [
    `I'm editing a Grapheme script in the Automations workbench.`,
    ``,
    `**${label}**`,
    idLine,
    ``,
    `\`\`\`grapheme`,
    excerptBody(body),
    `\`\`\``,
  ];

  if (input.compileError) {
    sections.push("", `Latest compile error: ${input.compileError}`);
  } else if (input.compileHints?.length) {
    sections.push("", "Compile notes:", ...input.compileHints.map((hint) => `- ${hint}`));
  }

  if (input.runError) {
    sections.push("", `Latest run error: ${input.runError}`);
  } else if (input.runResult) {
    const parsed = parseGraphemeRunResult(input.runResult);
    if (parsed?.summary) {
      sections.push("", "Latest run output:", parsed.summary.slice(0, 1200));
    }
  }

  sections.push(
    "",
    "Help me improve this script — explain modules, fix errors, or suggest the next step.",
  );
  return sections.join("\n");
}
