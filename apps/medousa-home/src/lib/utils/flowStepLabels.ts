import type { WorkflowStepSpec } from "$lib/types/workflow";

export function flowStepTitle(step: WorkflowStepSpec): string {
  if (step.kind === "prompt") {
    const text = step.user_prompt.trim();
    if (!text) return "Ask Medousa something";
    return text.length > 64 ? `${text.slice(0, 61)}…` : text;
  }

  if (step.kind === "grapheme") {
    if (step.script_name?.trim()) {
      return step.script_name.trim();
    }
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

/** Human sequence label — “Then: …” after the first step. */
export function flowStepSequenceLabel(index: number, step: WorkflowStepSpec): string {
  const title = flowStepTitle(step);
  if (index === 0) return title;
  return `Then: ${title}`;
}

export function flowStepSubtitle(step: WorkflowStepSpec): string {
  switch (step.kind) {
    case "prompt":
      return "Ask Medousa";
    case "grapheme":
      if (step.script_name?.trim()) {
        return `Script · ${step.script_name.trim()}`;
      }
      return "Grapheme script";
    case "mcp":
      return step.server_id.trim()
        ? `Tool · ${step.server_id}.${step.tool_name || "tool"}`
        : "External tool";
    case "tool_replay":
      return `Replay · ${step.tool_name}`;
    default:
      return "Step";
  }
}

export const GRAPHEME_STEP_PLACEHOLDER = `glyph Main {
  web.duckduckgo(query: "your question here", max_results: 3)
}`;
