import type { WorkflowStepSpec } from "$lib/types/workflow";

export function flowStepTitle(step: WorkflowStepSpec): string {
  if (step.kind === "prompt") {
    const text = step.user_prompt.trim();
    if (!text) return "Ask Medousa something";
    return text.length > 64 ? `${text.slice(0, 61)}…` : text;
  }

  if (step.kind === "grapheme") {
    const source = step.source.trim();
    if (!source) return "Run a Grapheme script";
    const call = source.match(/[\w.]+\([^)]*\)/)?.[0] ?? source.split("\n").find((line) => line.trim())?.trim();
    if (call && call.length <= 56) return call;
    if (source.includes("duckduckgo")) return "Search the web";
    if (source.includes("core.echo")) return "Show a message";
    return "Run your script";
  }

  if (step.kind === "mcp") {
    if (step.tool_name.trim()) {
      return `Use ${step.tool_name.replaceAll("_", " ")}`;
    }
    return "Connect an external tool";
  }

  if (step.kind === "tool_replay") {
    return `Do ${step.tool_name.replaceAll("_", " ")} again`;
  }

  return "Step";
}

export function flowStepSubtitle(step: WorkflowStepSpec): string {
  const meta = `${step.kind} · ${step.id}`;

  switch (step.kind) {
    case "prompt":
      return meta;
    case "grapheme":
      return `${meta} · Grapheme source`;
    case "mcp":
      return step.server_id.trim()
        ? `${meta} · ${step.server_id}.${step.tool_name || "tool"}`
        : meta;
    case "tool_replay":
      return `${meta} · ${step.tool_name}`;
    default:
      return meta;
  }
}

export const GRAPHEME_STEP_PLACEHOLDER = `glyph Main {
  web.duckduckgo(query: "your question here", max_results: 3)
}`;
