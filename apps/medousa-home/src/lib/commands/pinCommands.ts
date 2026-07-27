import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import {
  spotlightPins,
  SPOTLIGHT_PIN_SLOTS,
  type SpotlightPin,
} from "$lib/stores/spotlightPins.svelte";
import { formatSessionLabel } from "$lib/utils/formatSession";
import { vaultDisplayTitle } from "$lib/utils/formatVault";
import type { WorkshopCommand, WorkshopCommandContext } from "./types";

function pinKindLabel(kind: SpotlightPin["kind"]): string {
  switch (kind) {
    case "note":
      return "Note";
    case "chat":
      return "Chat";
    case "script":
      return "Script";
    case "surface":
      return "Place";
  }
}

async function jumpPin(ctx: WorkshopCommandContext, pin: SpotlightPin) {
  switch (pin.kind) {
    case "note": {
      await lmeWorkspace.openNote(pin.target);
      break;
    }
    case "chat": {
      await ctx.chat.switchSession(pin.target);
      ctx.callbacks.focusChat();
      break;
    }
    case "script": {
      await lmeWorkspace.openScriptById(pin.target);
      break;
    }
    case "surface": {
      ctx.navigate(pin.target as Parameters<WorkshopCommandContext["navigate"]>[0]);
      break;
    }
  }
  ctx.callbacks.close();
}

function currentPin(ctx: WorkshopCommandContext): SpotlightPin | null {
  const scriptTab = graphemeScriptEditor.activeTab;
  if (
    ctx.layout.desktopSurface === "automations" &&
    scriptTab?.scriptId
  ) {
    return {
      kind: "script",
      target: scriptTab.scriptId,
      label: scriptTab.name,
    };
  }
  if (scriptTab?.scriptId && ctx.layout.desktopSurface === "automations") {
    return {
      kind: "script",
      target: scriptTab.scriptId,
      label: scriptTab.name,
    };
  }
  if (ctx.vault.selectedPath && ctx.layout.desktopSurface === "library") {
    return {
      kind: "note",
      target: ctx.vault.selectedPath,
      label:
        vaultDisplayTitle(ctx.vault.title, ctx.vault.selectedPath) ||
        ctx.vault.selectedPath,
    };
  }
  if (ctx.vault.selectedPath) {
    return {
      kind: "note",
      target: ctx.vault.selectedPath,
      label:
        vaultDisplayTitle(ctx.vault.title, ctx.vault.selectedPath) ||
        ctx.vault.selectedPath,
    };
  }
  if (ctx.chat.sessionId) {
    const session = ctx.chat.sessions.find(
      (s) => s.session_id === ctx.chat.sessionId,
    );
    return {
      kind: "chat",
      target: ctx.chat.sessionId,
      label: session ? formatSessionLabel(session) : "Current chat",
    };
  }
  return {
    kind: "surface",
    target: ctx.layout.desktopSurface,
    label: `Go to ${ctx.layout.desktopSurface}`,
  };
}

export function buildPinnedJumpCommands(): WorkshopCommand[] {
  // Idempotent: only reloads localStorage when workshop id changes (never every collect).
  spotlightPins.ensureWorkshopSynced();
  const commands: WorkshopCommand[] = [];
  for (let i = 0; i < SPOTLIGHT_PIN_SLOTS; i += 1) {
    const pin = spotlightPins.get(i);
    if (!pin) continue;
    const slot = i + 1;
    commands.push({
      id: `pin-jump:${i}`,
      section: "pinned",
      verb: "pin",
      label: `${slot}. ${pin.label}`,
      subtitle: pinKindLabel(pin.kind),
      hint: String(slot),
      keywords: `pin jump ${slot} ${pin.label} ${pin.kind}`,
      aliases: [String(slot)],
      preview:
        pin.kind === "note"
          ? { kind: "note", path: pin.target }
          : pin.kind === "script"
            ? { kind: "script", scriptId: pin.target }
            : pin.kind === "chat"
              ? { kind: "chat", sessionId: pin.target }
              : { kind: "text", text: pin.label },
      run: async (ctx) => {
        await jumpPin(ctx, pin);
      },
    });
  }
  return commands;
}

export function buildPinManageCommands(
  ctx: WorkshopCommandContext,
): WorkshopCommand[] {
  const current = currentPin(ctx);
  const commands: WorkshopCommand[] = [
    {
      id: "do-pin-current",
      section: "do",
      verb: "pin",
      label: current
        ? `Pin current · ${current.label}`
        : "Pin current",
      subtitle: "Add to working set (slots 1–4)",
      keywords: "pin harpoon mark current working set",
      aliases: ["pin", "harpoon"],
      run: (runCtx) => {
        const pin = currentPin(runCtx);
        if (!pin) {
          runCtx.error("Nothing to pin.");
          return;
        }
        const slot = spotlightPins.pin(pin);
        runCtx.callbacks.close();
        runCtx.notice(`Pinned in slot ${slot + 1}.`);
      },
    },
    {
      id: "do-pin-clear",
      section: "do",
      verb: "pin",
      label: "Clear all pins",
      subtitle: "Empty the working set",
      keywords: "pin clear unpin all",
      risk: "attention",
      run: (runCtx) => {
        spotlightPins.clear();
        runCtx.callbacks.close();
        runCtx.notice("Pins cleared.");
      },
    },
  ];

  for (let i = 0; i < SPOTLIGHT_PIN_SLOTS; i += 1) {
    const pin = spotlightPins.get(i);
    if (!pin) continue;
    commands.push({
      id: `do-pin-unpin:${i}`,
      section: "do",
      verb: "pin",
      label: `Unpin ${i + 1} · ${pin.label}`,
      subtitle: "Remove from working set",
      keywords: `unpin pin ${i + 1}`,
      run: (runCtx) => {
        spotlightPins.unpin(i);
        runCtx.callbacks.close();
        runCtx.notice(`Unpinned slot ${i + 1}.`);
      },
    });
  }

  return commands;
}

export function jumpPinSlot(
  ctx: WorkshopCommandContext,
  slotIndex: number,
): boolean {
  const pin = spotlightPins.get(slotIndex);
  if (!pin) return false;
  void jumpPin(ctx, pin);
  return true;
}
