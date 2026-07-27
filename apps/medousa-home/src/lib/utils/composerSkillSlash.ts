/** Slash token + menu items for composer skill/tool attachment. */

import type { CapabilityListEntry, ManuscriptCatalogEntry } from "$lib/types/catalog";
import { filterSkills } from "$lib/utils/skillCatalog";
import { filterTools } from "$lib/utils/toolCatalog";
import { SLASH_COMMAND_HINTS } from "$lib/utils/slashCommands";

export type ComposerSlashItem =
  | {
      kind: "command";
      id: string;
      label: string;
      hint: string;
      keywords: string;
      insert: string;
    }
  | {
      kind: "skill";
      id: string;
      label: string;
      hint: string;
      keywords: string;
    }
  | {
      kind: "tool";
      id: string;
      label: string;
      hint: string;
      keywords: string;
    };

export type ComposerSlashToken = {
  start: number;
  end: number;
  /** Text after `/`, lowercased. */
  filter: string;
  raw: string;
};

const TOKEN_RE = /(^|[\s])(\/[\w-]*)$/;

export function composerSlashToken(
  value: string,
  cursor: number,
): ComposerSlashToken | null {
  if (cursor < 0 || cursor > value.length) return null;
  const before = value.slice(0, cursor);
  const match = before.match(TOKEN_RE);
  if (!match) return null;
  const raw = match[2] ?? "";
  if (!raw.startsWith("/")) return null;
  const start = before.length - raw.length;
  return {
    start,
    end: cursor,
    filter: raw.slice(1).toLowerCase(),
    raw,
  };
}

export function stripComposerSlashToken(
  value: string,
  token: ComposerSlashToken,
  insert = "",
): { value: string; cursor: number } {
  const next = value.slice(0, token.start) + insert + value.slice(token.end);
  const cursor = token.start + insert.length;
  return { value: next, cursor };
}

const COMMAND_ITEMS: ComposerSlashItem[] = [
  {
    kind: "command",
    id: "cmd-ask",
    label: "/ask",
    hint: "Background work job",
    keywords: "ask background job work",
    insert: "/ask ",
  },
  {
    kind: "command",
    id: "cmd-budget-list",
    label: "/budget list",
    hint: "Pending round approvals",
    keywords: "budget list approve",
    insert: "/budget list",
  },
  {
    kind: "command",
    id: "cmd-usage",
    label: "/usage",
    hint: "Context window breakdown",
    keywords: "usage context tokens",
    insert: "/usage",
  },
  {
    kind: "command",
    id: "cmd-help",
    label: "/help",
    hint: "Show commands",
    keywords: "help commands",
    insert: "/help",
  },
];

function matchesFilter(filter: string, ...parts: string[]): boolean {
  if (!filter) return true;
  const hay = parts.join(" ").toLowerCase();
  return hay.includes(filter);
}

export function buildComposerSlashItems(options: {
  filter: string;
  manuscripts: ManuscriptCatalogEntry[];
  capabilities: CapabilityListEntry[];
  attachedSkillIds: string[];
  attachedToolIds: string[];
  /** Include chat slash commands (Chat host). Ask dock can omit. */
  includeCommands?: boolean;
  skillLimit?: number;
  toolLimit?: number;
}): ComposerSlashItem[] {
  const filter = options.filter.trim().toLowerCase();
  const skillLimit = options.skillLimit ?? 8;
  const toolLimit = options.toolLimit ?? 8;
  const items: ComposerSlashItem[] = [];

  // Skills first (Cursor-style), then commands, then tools.
  const skills = filterSkills(options.manuscripts, filter, "runnable")
    .filter((entry) => !options.attachedSkillIds.includes(entry.id))
    .slice(0, skillLimit);
  for (const entry of skills) {
    items.push({
      kind: "skill",
      id: entry.id,
      label: entry.name,
      hint: entry.description?.trim() || entry.id,
      keywords: `${entry.id} skill manuscript`,
    });
  }

  if (options.includeCommands !== false) {
    for (const command of COMMAND_ITEMS) {
      if (
        matchesFilter(
          filter,
          command.label,
          command.hint,
          command.keywords,
          command.kind === "command" ? command.insert : "",
        )
      ) {
        items.push(command);
      }
    }
  }

  const tools = filterTools(options.capabilities, filter, "all")
    .filter((entry) => !options.attachedToolIds.includes(entry.id))
    .slice(0, toolLimit);
  for (const entry of tools) {
    items.push({
      kind: "tool",
      id: entry.id,
      label: entry.title,
      hint: entry.description?.trim() || entry.id,
      keywords: `${entry.id} tool capability`,
    });
  }

  void SLASH_COMMAND_HINTS;
  return items;
}

export function groupComposerSlashItems(items: ComposerSlashItem[]): {
  commands: ComposerSlashItem[];
  skills: ComposerSlashItem[];
  tools: ComposerSlashItem[];
} {
  // Preserve encounter order within each kind (skills/tools already filtered).
  return {
    skills: items.filter((item) => item.kind === "skill"),
    commands: items.filter((item) => item.kind === "command"),
    tools: items.filter((item) => item.kind === "tool"),
  };
}
