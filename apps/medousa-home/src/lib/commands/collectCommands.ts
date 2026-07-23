import {
  buildAdvancedCommands,
  buildAskCommands,
  buildBrowserCommands,
  buildGoCommands,
  buildLibraryCommands,
  buildPaneCommands,
  buildTuneCommands,
  buildWorkspaceCommands,
} from "./registry";
import { buildSuggestedCommands, buildBudgetListCommand } from "./contextCommands";
import {
  buildNoteOpenCommands,
  buildBrowserHistoryCommands,
  buildSessionOpenCommands,
  buildWorkCardOpenCommands,
  buildRecentSessionCommands,
} from "./searchProviders";
import { filterAndSortCommands } from "./score";
import type { CommandSection, GroupedCommands, WorkshopCommand, WorkshopCommandContext } from "./types";
import { SECTION_LABELS as LABELS, SECTION_ORDER as ORDER } from "./types";

export interface CollectCommandsOptions {
  query: string;
  notesMode?: boolean;
}

function dedupeCommandsById(commands: WorkshopCommand[]): WorkshopCommand[] {
  const seen = new Set<string>();
  const unique: WorkshopCommand[] = [];
  for (const command of commands) {
    if (seen.has(command.id)) continue;
    seen.add(command.id);
    unique.push(command);
  }
  return unique;
}

function groupCommands(commands: WorkshopCommand[]): GroupedCommands[] {
  const buckets = new Map<CommandSection, WorkshopCommand[]>();
  for (const command of commands) {
    const list = buckets.get(command.section) ?? [];
    list.push(command);
    buckets.set(command.section, list);
  }
  return ORDER.filter((section) => buckets.has(section)).map((section) => ({
    section,
    label: LABELS[section],
    commands: buckets.get(section) ?? [],
  }));
}

export function collectWorkshopCommands(
  ctx: WorkshopCommandContext,
  options: CollectCommandsOptions,
): GroupedCommands[] {
  let rawQuery = options.query.trim();
  const showAdvanced = rawQuery.startsWith(">");
  if (showAdvanced) {
    rawQuery = rawQuery.slice(1).trim();
  }

  const suggested = buildSuggestedCommands(ctx);
  const budgetList = buildBudgetListCommand(ctx);
  if (budgetList) suggested.push(budgetList);

  const staticPool: WorkshopCommand[] = [
    ...suggested,
    ...buildGoCommands(),
    ...buildWorkspaceCommands(),
    ...buildAskCommands(),
    ...buildTuneCommands(),
    ...buildBrowserCommands(),
    ...buildLibraryCommands(),
    ...buildPaneCommands(),
  ];

  if (showAdvanced || rawQuery.length > 0) {
    staticPool.push(...buildAdvancedCommands());
  }

  const searchPool: WorkshopCommand[] = [
    ...buildNoteOpenCommands(ctx, rawQuery),
    ...buildSessionOpenCommands(ctx, rawQuery),
    ...buildWorkCardOpenCommands(ctx, rawQuery),
    ...buildBrowserHistoryCommands(rawQuery),
  ];

  let pool: WorkshopCommand[];

  if (options.notesMode && !rawQuery) {
    pool = [
      ...buildNoteOpenCommands(ctx, ""),
      ...buildGoCommands().filter((c) => c.id === "go-library"),
    ];
  } else if (!rawQuery && suggested.some((c) => !c.id.startsWith("open-session:"))) {
    pool = [...staticPool, ...buildNoteOpenCommands(ctx, "").slice(0, 8)];
  } else if (!rawQuery) {
    pool = [...buildGoCommands(), ...buildRecentSessionCommands(ctx), ...buildAskCommands().slice(0, 2), ...buildBrowserHistoryCommands("")];
  } else {
    pool = [...staticPool, ...searchPool];
  }

  const filtered = filterAndSortCommands(dedupeCommandsById(pool), rawQuery, 64);
  return groupCommands(filtered);
}

export function flattenGroups(groups: GroupedCommands[]): WorkshopCommand[] {
  return groups.flatMap((group) => group.commands);
}

export function findWorkshopCommandById(
  ctx: WorkshopCommandContext,
  commandId: string,
  options: CollectCommandsOptions,
): WorkshopCommand | undefined {
  return flattenGroups(collectWorkshopCommands(ctx, options)).find(
    (command) => command.id === commandId,
  );
}
