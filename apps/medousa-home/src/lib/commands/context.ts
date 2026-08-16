import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { layout } from "$lib/runtime/layout.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settingsNav } from "$lib/stores/settingsNav.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import type { SettingsSectionId } from "$lib/types/settings";
import type { RuntimeTab } from "$lib/types/runtime";
import type { Surface } from "$lib/types/ui";
import type { CommandSpotlightCallbacks, WorkshopCommandContext } from "./types";

export function buildWorkshopCommandContext(
  callbacks: CommandSpotlightCallbacks,
): WorkshopCommandContext {
  return {
    layout,
    chat,
    workspace,
    vault,
    runtime,
    connection,
    settingsNav,
    callbacks,
    navigate(surface: Surface) {
      // Same path as WorkshopShell: open/activate a real shell tab when one
      // exists. LME families (notes/code/…) only seed empty Workspace/Code
      // placeholders when a document is opened — never on rail entry.
      if (surface === "context") {
        shellTabs.openSurface("map", { activate: true });
        return;
      }
      if (surface === "automations") {
        const mode = lmeWorkspace.explorerMode;
        if (
          mode !== "scripts" &&
          mode !== "flows" &&
          mode !== "schedules" &&
          mode !== "history" &&
          mode !== "agents"
        ) {
          lmeWorkspace.setExplorerMode("scripts");
        }
        shellTabs.enterLmeFamily("library");
        return;
      }
      if (surface === "workshop") {
        lmeWorkspace.setExplorerMode("agents");
        shellTabs.enterLmeFamily("library");
        return;
      }
      if (
        surface === "notes" ||
        surface === "files" ||
        surface === "artifacts" ||
        surface === "library"
      ) {
        if (surface === "notes") lmeWorkspace.setExplorerMode("notes");
        if (surface === "files") lmeWorkspace.setExplorerMode("files");
        if (surface === "artifacts") lmeWorkspace.setExplorerMode("artifacts");
        shellTabs.enterLmeFamily("library");
        return;
      }
      if (surface === "code") {
        lmeWorkspace.setExplorerMode("code");
        shellTabs.enterLmeFamily("code");
        return;
      }
      if (surface === "chat") {
        void chat.refreshSessions();
        void chat.ensureSessionHydrated();
        const sessionId = chat.sessionId?.trim();
        if (sessionId) {
          shellTabs.openChat(sessionId, { activate: true });
        } else {
          shellTabs.openSurface("chat", { activate: true });
        }
        return;
      }
      if (surface === "work") {
        void workspace.prefetchCardDetails();
      }
      shellTabs.openDestination(surface);
    },
    openRuntimeTab(tab: RuntimeTab) {
      runtime.activeTab = tab;
      shellTabs.openDestination("runtime");
    },
    openSettingsSection(section: SettingsSectionId) {
      settingsNav.openSection(section);
      shellTabs.openDestination("settings");
      layout.openShellSidebarView("settings");
    },
    notice(message: string) {
      chat.historyNotice = message;
    },
    error(message: string) {
      chat.setError(message);
    },
  };
}
