import { workshop } from "$lib/stores/workshop.svelte";
import { spotlightPins } from "$lib/stores/spotlightPins.svelte";
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
import { buildDoCommands, buildScriptRunOpenCommands } from "./doCommands";
import {
  buildPinManageCommands,
  buildPinnedJumpCommands,
} from "./pinCommands";
import {
  buildNoteOpenCommands,
  buildBrowserHistoryCommands,
  buildSessionOpenCommands,
  buildWorkCardOpenCommands,
  buildRecentSessionCommands,
} from "./searchProviders";
import { filterAndSortCommands } from "./score";
import type {
  CommandSection,
  CommandVerb,
  GroupedCommands,
  WorkshopCommand,
  WorkshopCommandContext,
} from "./types";
import { SECTION_LABELS as LABELS, SECTION_ORDER as ORDER } from "./types";

export type SpotlightPrefixMode = "default" | "advanced" | "create" | "run";

export interface CollectCommandsOptions {
  query: string;
  notesMode?: boolean;
}

export interface ParsedSpotlightQuery {
  mode: SpotlightPrefixMode;
  rawQuery: string;
  /** Original trimmed input (with prefix). */
  input: string;
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

export function parseSpotlightQuery(query: string): ParsedSpotlightQuery {
  const input = query.trim();
  if (input.startsWith(">")) {
    return { mode: "advanced", rawQuery: input.slice(1).trim(), input };
  }
  if (input.startsWith("+")) {
    return { mode: "create", rawQuery: input.slice(1).trim(), input };
  }
  if (input.startsWith("!")) {
    return { mode: "run", rawQuery: input.slice(1).trim(), input };
  }
  return { mode: "default", rawQuery: input, input };
}

function filterByVerb(
  commands: WorkshopCommand[],
  verbs: CommandVerb[],
): WorkshopCommand[] {
  const allowed = new Set(verbs);
  return commands.filter((c) => c.verb && allowed.has(c.verb));
}

function withNotePreviews(commands: WorkshopCommand[]): WorkshopCommand[] {
  return commands.map((command) => {
    if (command.preview || !command.id.startsWith("open-note:")) return command;
    const path = command.id.slice("open-note:".length);
    return {
      ...command,
      preview: { kind: "note", path },
    };
  });
}

export function collectWorkshopCommands(
  ctx: WorkshopCommandContext,
  options: CollectCommandsOptions,
): GroupedCommands[] {
  const parsed = parseSpotlightQuery(options.query);
  const { mode, rawQuery } = parsed;
  const showAdvanced = mode === "advanced" || rawQuery.length > 0;

  const suggested = buildSuggestedCommands(ctx);
  const budgetList = buildBudgetListCommand(ctx);
  if (budgetList) suggested.push(budgetList);

  const doCommands = [
    ...buildDoCommands(),
    ...buildPinManageCommands(ctx),
  ];
  const pinned = buildPinnedJumpCommands();

  if (mode === "create") {
    const pool = filterByVerb(doCommands, ["create"]);
    const filtered = filterAndSortCommands(dedupeCommandsById(pool), rawQuery, 64);
    return groupCommands(filtered);
  }

  if (mode === "run") {
    // Sync run verbs + async script hits are merged by caller when needed;
    // collect stays sync — script fuzzy commands built below via cached workshop.scripts.
    const runPool = [
      ...filterByVerb(doCommands, ["run"]),
      ...buildRunScriptHitsFromCache(rawQuery),
    ];
    const filtered = filterAndSortCommands(dedupeCommandsById(runPool), rawQuery, 64);
    return groupCommands(filtered);
  }

  const staticPool: WorkshopCommand[] = [
    ...suggested,
    ...pinned,
    ...buildGoCommands(),
    ...buildWorkspaceCommands(),
    ...doCommands,
    ...buildAskCommands(),
    ...buildTuneCommands(),
    ...buildBrowserCommands(),
    ...buildLibraryCommands(),
    ...buildPaneCommands(),
  ];

  if (showAdvanced) {
    staticPool.push(...buildAdvancedCommands());
  }

  const searchPool: WorkshopCommand[] = withNotePreviews([
    ...buildNoteOpenCommands(ctx, rawQuery),
    ...buildSessionOpenCommands(ctx, rawQuery),
    ...buildWorkCardOpenCommands(ctx, rawQuery),
    ...buildBrowserHistoryCommands(rawQuery),
  ]);

  let pool: WorkshopCommand[];

  if (options.notesMode && !rawQuery) {
    pool = withNotePreviews([
      ...buildNoteOpenCommands(ctx, ""),
      ...buildGoCommands().filter((c) => c.id === "go-library"),
    ]);
  } else if (!rawQuery && suggested.some((c) => !c.id.startsWith("open-session:"))) {
    pool = [
      ...pinned,
      ...staticPool,
      ...withNotePreviews(buildNoteOpenCommands(ctx, "").slice(0, 8)),
    ];
  } else if (!rawQuery) {
    pool = [
      ...pinned,
      ...buildGoCommands(),
      ...buildRecentSessionCommands(ctx),
      ...doCommands.slice(0, 6),
      ...buildAskCommands().slice(0, 2),
      ...buildBrowserHistoryCommands(""),
    ];
  } else {
    pool = [...staticPool, ...searchPool];
  }

  const filtered = filterAndSortCommands(dedupeCommandsById(pool), rawQuery, 64);
  return groupCommands(filtered);
}

function buildRunScriptHitsFromCache(query: string): WorkshopCommand[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return [];
  const scripts = workshop.scripts ?? [];
  return scripts
    .filter((script) => {
      const hay = `${script.name} ${script.tags?.join(" ") ?? ""}`.toLowerCase();
      return hay.includes(trimmed) || script.id.toLowerCase().includes(trimmed);
    })
    .slice(0, 12)
    .map((script) => ({
      id: `do-run-script:${script.id}`,
      section: "do" as const,
      verb: "run" as const,
      label: `Run ${script.name}`,
      subtitle: script.tags?.length ? script.tags.join(" · ") : "Saved script",
      keywords: `run script ${script.name}`,
      aliases: [script.name.toLowerCase()],
      preview: {
        kind: "script" as const,
        scriptId: script.id,
        ...(script.body_preview
          ? { body: script.body_preview }
          : {}),
      },
      run: async (runCtx: WorkshopCommandContext) => {
        const { getGraphemeScript } = await import("$lib/daemon");
        const detail = await getGraphemeScript(script.id);
        const source = detail.body_preview?.trim() ?? "";
        if (!source) {
          runCtx.error("Couldn’t load that script’s source.");
          return;
        }
        spotlightPins.setLastScriptId(script.id);
        await workshop.runScriptSource(source);
        runCtx.navigate("automations");
        runCtx.callbacks.close();
        runCtx.notice(
          workshop.runError
            ? `Script failed: ${workshop.runError}`
            : `Ran ${detail.script.name}.`,
        );
      },
    }));
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

export { buildScriptRunOpenCommands };
