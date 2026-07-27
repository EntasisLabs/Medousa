import {
  contentZoomPercent,
  resetContentZoom,
  stepContentZoom,
} from "$lib/config/contentZoom";
import { enqueueDaemonAsk } from "$lib/daemon";
import {
  applyRecipeToEditor,
  GRAPHEME_STARTER_RECIPES,
  recipeById,
} from "$lib/grapheme/graphemeRecipes";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { spotlightPins } from "$lib/stores/spotlightPins.svelte";
import { workshop } from "$lib/stores/workshop.svelte";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";

async function runScriptById(
  ctx: WorkshopCommandContext,
  scriptId: string,
): Promise<void> {
  const { getGraphemeScript } = await import("$lib/daemon");
  const detail = await getGraphemeScript(scriptId);
  const source = detail.body_preview?.trim() ?? "";
  if (!source) {
    ctx.error("Couldn’t load that script’s source.");
    return;
  }
  spotlightPins.setLastScriptId(scriptId);
  await workshop.runScriptSource(source);
  ctx.navigate("automations");
  ctx.callbacks.close();
  ctx.notice(
    workshop.runError
      ? `Script failed: ${workshop.runError}`
      : `Ran ${detail.script.name}.`,
  );
}

function buildCreateCommands(): WorkshopCommand[] {
  const templateCommands: WorkshopCommand[] = GRAPHEME_STARTER_RECIPES.map(
    (recipe) => ({
      id: `do-create-script-${recipe.id}`,
      section: "do" as const,
      verb: "create" as const,
      label: `New script · ${recipe.title}`,
      subtitle: recipe.subtitle,
      keywords: `create new script template ${recipe.title} ${recipe.id} grapheme`,
      aliases: [recipe.id, recipe.scriptName.toLowerCase()],
      preview: { kind: "text", text: recipe.body },
      run: async (ctx) => {
        const applied = applyRecipeToEditor(recipe);
        lmeWorkspace.openNewScript();
        graphemeScriptEditor.patchActiveTab({
          name: applied.name,
          body: applied.body,
          intent: applied.intent,
        });
        ctx.navigate("automations");
        ctx.callbacks.close();
        ctx.notice(`Opened template: ${recipe.title}`);
      },
    }),
  );

  return [
    {
      id: "do-create-note",
      section: "do",
      verb: "create",
      label: "New note",
      subtitle: "Create a note in the Library",
      keywords: "create new note vault library write",
      aliases: ["note", "+note", "nn"],
      prompt: {
        placeholder: "Note title…",
        submitLabel: "Create",
      },
      run: async (ctx, title) => {
        const name = title?.trim() || "Untitled";
        const spaceId = ctx.vault.activeSpace?.id ?? "journal";
        const path = await ctx.vault.createNote({
          spaceId,
          title: name,
          open: true,
        });
        if (path) {
          await lmeWorkspace.openNote(path);
          ctx.callbacks.close();
          ctx.notice(`Created “${name}”.`);
        } else {
          ctx.error(ctx.vault.error ?? "Couldn’t create note.");
        }
      },
    },
    {
      id: "do-create-chat",
      section: "do",
      verb: "create",
      label: "New chat",
      subtitle: "Start a fresh conversation",
      keywords: "create new chat session conversation",
      aliases: ["chat", "+chat"],
      run: async (ctx) => {
        await ctx.chat.newSession();
        ctx.navigate("chat");
        ctx.callbacks.focusChat();
        ctx.callbacks.close();
        ctx.notice("Started a new conversation.");
      },
    },
    {
      id: "do-create-script",
      section: "do",
      verb: "create",
      label: "New blank script",
      subtitle: "Open Scripts workbench",
      keywords: "create new script blank grapheme automations",
      aliases: ["script", "+script"],
      run: (ctx) => {
        lmeWorkspace.openNewScript();
        ctx.navigate("automations");
        ctx.callbacks.close();
        ctx.notice("New script tab opened.");
      },
    },
    ...templateCommands,
  ];
}

function buildRunCommands(): WorkshopCommand[] {
  return [
    {
      id: "do-run-last-script",
      section: "do",
      verb: "run",
      label: "Run last script",
      subtitle: spotlightPins.lastScriptId
        ? "Re-run the script you ran last"
        : "No script run yet this session",
      keywords: "run last script again grapheme",
      aliases: ["!!", "last"],
      run: async (ctx) => {
        const id = spotlightPins.lastScriptId;
        if (!id) {
          ctx.error("No last script — run one from Automations or ! search.");
          return;
        }
        await runScriptById(ctx, id);
      },
    },
    {
      id: "do-run-script",
      section: "do",
      verb: "run",
      label: "Run script…",
      subtitle: "Pick a saved script by name",
      keywords: "run script grapheme library execute",
      aliases: ["run"],
      prompt: {
        placeholder: "Script name…",
        submitLabel: "Run",
      },
      run: async (ctx, query) => {
        const q = query?.trim() ?? "";
        const { listGraphemeScripts } = await import("$lib/daemon");
        const listed = await listGraphemeScripts({
          query: q || undefined,
          limit: 24,
        });
        const scripts = listed.scripts ?? [];
        if (scripts.length === 0) {
          ctx.error(q ? `No scripts match “${q}”.` : "No saved scripts yet.");
          return;
        }
        const lower = q.toLowerCase();
        const exact = scripts.find(
          (s) => s.name.toLowerCase() === lower || s.id === q,
        );
        const pick = exact ?? scripts[0];
        await runScriptById(ctx, pick.id);
      },
    },
    {
      id: "do-run-morning-brief",
      section: "do",
      verb: "run",
      label: "Morning brief",
      subtitle: "Queue the morning-brief manuscript",
      keywords: "run morning brief digest summary",
      aliases: ["brief", "morning"],
      run: async (ctx) => {
        await enqueueDaemonAsk({
          prompt: "Run the morning brief.",
          manuscriptId: "morning-brief",
        });
        ctx.navigate("work");
        ctx.callbacks.close();
        ctx.notice("Morning brief queued.");
      },
    },
  ];
}

function buildToggleCommands(): WorkshopCommand[] {
  return [
    {
      id: "do-toggle-note-plane",
      section: "do",
      verb: "toggle",
      label: "Toggle Live / Build",
      subtitle: "Switch note plane",
      keywords: "toggle live build plane editor write",
      aliases: ["live", "build", "plane"],
      run: (ctx) => {
        ctx.vault.toggleNotePlane();
        ctx.callbacks.close();
        ctx.notice(
          ctx.vault.notePlane === "live" ? "Live plane" : "Build plane",
        );
      },
    },
    {
      id: "do-toggle-preview",
      section: "do",
      verb: "toggle",
      label: "Toggle Preview / Edit",
      subtitle: "Note editor mode",
      keywords: "toggle preview edit view mode",
      aliases: ["preview", "edit"],
      run: (ctx) => {
        if (ctx.vault.editorMode === "preview") {
          ctx.vault.enterEditMode();
          ctx.notice("Edit mode");
        } else {
          ctx.vault.enterPreviewMode();
          ctx.notice("Preview mode");
        }
        ctx.callbacks.close();
      },
    },
    {
      id: "do-toggle-split",
      section: "do",
      verb: "toggle",
      label: "Toggle split preview",
      subtitle: "Build pane beside source",
      keywords: "toggle split preview pane side",
      aliases: ["split"],
      run: (ctx) => {
        ctx.layout.toggleVaultSplitEnabled();
        ctx.callbacks.close();
        ctx.notice(
          ctx.layout.vaultSplitEnabled ? "Split preview on" : "Split preview off",
        );
      },
    },
    {
      id: "do-toggle-links",
      section: "do",
      verb: "toggle",
      label: "Toggle links panel",
      subtitle: "Wikilinks and backlinks",
      keywords: "toggle links panel backlinks wikilinks",
      aliases: ["links"],
      run: (ctx) => {
        ctx.layout.toggleVaultLinksPanel();
        ctx.callbacks.close();
        ctx.notice(
          ctx.layout.vaultLinksPanelOpen ? "Links panel open" : "Links panel closed",
        );
      },
    },
    {
      id: "do-zoom-in",
      section: "do",
      verb: "toggle",
      label: "Zoom in",
      subtitle: "Whole UI larger",
      keywords: "zoom in larger scale ui",
      run: (ctx) => {
        const zoom = stepContentZoom(1);
        ctx.callbacks.close();
        ctx.notice(`Zoom ${contentZoomPercent(zoom)}`);
      },
    },
    {
      id: "do-zoom-out",
      section: "do",
      verb: "toggle",
      label: "Zoom out",
      subtitle: "Whole UI smaller",
      keywords: "zoom out smaller scale ui",
      run: (ctx) => {
        const zoom = stepContentZoom(-1);
        ctx.callbacks.close();
        ctx.notice(`Zoom ${contentZoomPercent(zoom)}`);
      },
    },
    {
      id: "do-zoom-reset",
      section: "do",
      verb: "toggle",
      label: "Reset zoom",
      subtitle: "Back to 100%",
      keywords: "zoom reset 100 default",
      run: (ctx) => {
        resetContentZoom();
        ctx.callbacks.close();
        ctx.notice("Zoom 100%");
      },
    },
  ];
}

/** Dynamic run-by-name hits for `!` prefix / fuzzy. */
export async function buildScriptRunOpenCommands(
  query: string,
): Promise<WorkshopCommand[]> {
  const trimmed = query.trim();
  if (!trimmed) return [];
  try {
    const { listGraphemeScripts } = await import("$lib/daemon");
    const listed = await listGraphemeScripts({ query: trimmed, limit: 12 });
    return (listed.scripts ?? []).map((script) => ({
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
        ...(script.body_preview ? { body: script.body_preview } : {}),
      },
      run: async (ctx: WorkshopCommandContext) => {
        await runScriptById(ctx, script.id);
      },
    }));
  } catch {
    return [];
  }
}

function buildResumeCommand(): WorkshopCommand[] {
  return [
    {
      id: "do-resume-spotlight",
      section: "do",
      verb: "toggle",
      label: "Resume last Spotlight",
      subtitle: "Restore previous search (Telescope-style)",
      keywords: "resume last spotlight picker telescope",
      aliases: ["resume"],
      run: async (ctx) => {
        const { commandSpotlight } = await import(
          "$lib/stores/commandSpotlight.svelte"
        );
        if (!commandSpotlight.lastQuery.trim() && commandSpotlight.lastMode === "default") {
          ctx.notice("Nothing to resume yet.");
          return;
        }
        commandSpotlight.restoreLastQuery();
        ctx.notice("Restored last Spotlight query.");
      },
    },
  ];
}

export function buildDoCommands(): WorkshopCommand[] {
  return [
    ...buildCreateCommands(),
    ...buildRunCommands(),
    ...buildToggleCommands(),
    ...buildResumeCommand(),
  ];
}

export { recipeById };
