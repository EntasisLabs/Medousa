import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settingsNav } from "$lib/stores/settingsNav.svelte";
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
      layout.navigateDesktop(surface, { bump: true });
      if (surface === "chat") {
        void chat.refreshSessions();
        void chat.ensureSessionHydrated();
      }
      if (surface === "work") {
        void workspace.prefetchCardDetails();
      }
    },
    openRuntimeTab(tab: RuntimeTab) {
      runtime.activeTab = tab;
      layout.navigateDesktop("runtime", { bump: true });
    },
    openSettingsSection(section: SettingsSectionId) {
      settingsNav.openSection(section);
      layout.navigateDesktop("settings", { bump: true });
    },
    notice(message: string) {
      chat.historyNotice = message;
    },
    error(message: string) {
      chat.setError(message);
    },
  };
}
