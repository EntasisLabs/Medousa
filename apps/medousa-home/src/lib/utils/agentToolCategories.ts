/** Group cognition.* palette tools into module → action categories. */

export interface AgentToolEntry {
  id: string;
  moduleId: string;
  moduleLabel: string;
  actionId: string;
  actionLabel: string;
}

export interface AgentToolCategory {
  moduleId: string;
  label: string;
  tools: AgentToolEntry[];
  selectedCount: number;
}

const MODULE_LABELS: Record<string, string> = {
  turn: "Turn",
  artifact: "Artifacts",
  calendar: "Calendar",
  identity: "Identity",
  memory: "Memory",
  vault: "Vault",
  shell: "Shell",
  openshell: "OpenShell",
  skill: "Skills",
  utility: "Utility",
  browser: "Browser",
  feed: "Feeds",
  workshop: "Workshop",
  locus: "Locus",
  delivery: "Delivery",
  grapheme: "Grapheme",
  web: "Web",
};

function titleCase(raw: string): string {
  return raw
    .split(/[._\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function moduleLabel(moduleId: string): string {
  return MODULE_LABELS[moduleId] ?? titleCase(moduleId);
}

function actionLabel(actionId: string): string {
  return titleCase(actionId.replaceAll("_", " "));
}

/** Strip leading cognition. / cognition_ prefixes for grouping. */
export function parseAgentToolId(toolId: string): AgentToolEntry {
  const id = toolId.trim();
  let rest = id;
  if (rest.startsWith("cognition.")) {
    rest = rest.slice("cognition.".length);
  } else if (rest.startsWith("cognition_")) {
    rest = rest.slice("cognition_".length);
  }

  let moduleId: string;
  let actionId: string;

  if (rest.includes(".")) {
    const [mod, ...actionParts] = rest.split(".");
    moduleId = (mod || "other").toLowerCase();
    actionId = actionParts.join(".") || rest;
  } else if (rest.includes("_")) {
    const [mod, ...actionParts] = rest.split("_");
    moduleId = (mod || "other").toLowerCase();
    actionId = actionParts.join("_") || rest;
  } else {
    moduleId = "other";
    actionId = rest || id;
  }

  return {
    id,
    moduleId,
    moduleLabel: moduleLabel(moduleId),
    actionId,
    actionLabel: actionLabel(actionId),
  };
}

export function groupAgentTools(
  toolIds: string[],
  selected: string[],
  search = "",
): AgentToolCategory[] {
  const needle = search.trim().toLowerCase();
  const selectedSet = new Set(selected);
  const byModule = new Map<string, AgentToolEntry[]>();

  for (const toolId of toolIds) {
    const entry = parseAgentToolId(toolId);
    if (needle) {
      const hay = `${entry.moduleLabel} ${entry.actionLabel} ${entry.id}`.toLowerCase();
      if (!hay.includes(needle)) continue;
    }
    const list = byModule.get(entry.moduleId) ?? [];
    list.push(entry);
    byModule.set(entry.moduleId, list);
  }

  const categories: AgentToolCategory[] = [];
  for (const [moduleId, tools] of byModule) {
    tools.sort((a, b) => a.actionLabel.localeCompare(b.actionLabel));
    categories.push({
      moduleId,
      label: moduleLabel(moduleId),
      tools,
      selectedCount: tools.filter((tool) => selectedSet.has(tool.id)).length,
    });
  }

  categories.sort((a, b) => {
    if (a.moduleId === "other") return 1;
    if (b.moduleId === "other") return -1;
    return a.label.localeCompare(b.label);
  });
  return categories;
}
