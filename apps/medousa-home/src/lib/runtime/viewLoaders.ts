import type { FeatureId, FeatureInstance } from "./features/types";
import { loadFeature, loadedFeature } from "./features/loader";
import { probeClientPlatform } from "./platformProbe";

export type FeatureViewModule = { default: unknown };

type ViewInstance = FeatureInstance & { view: FeatureViewModule };

function catalogLoader(
  id: FeatureId,
  importer: () => Promise<FeatureViewModule>,
): () => Promise<FeatureViewModule> {
  return async () => {
    const existing = loadedFeature(id) as ViewInstance | undefined;
    if (existing?.view) return existing.view;
    await loadFeature(
      id,
      async () => ({
        async start() {
          const view = await importer();
          return {
            view,
            dispose() {},
          } satisfies ViewInstance;
        },
      }),
      { platform: probeClientPlatform() },
    );
    const loaded = loadedFeature(id) as ViewInstance | undefined;
    if (!loaded?.view) throw new Error(`feature ${id} produced no view`);
    return loaded.view;
  };
}

export const loadLmePanel = catalogLoader(
  "vault-browse",
  () => import("$lib/components/lme/LmePanel.svelte"),
);
export const loadWorkPanel = catalogLoader(
  "code-work",
  () => import("$lib/components/work/WorkPanel.svelte"),
);
export const loadHumanBrowserPanel = catalogLoader(
  "browser",
  () => import("$lib/components/browser/HumanBrowserPanel.svelte"),
);
export const loadSettingsPanel = catalogLoader(
  "settings",
  () => import("$lib/components/layout/SettingsPanel.svelte"),
);
export const loadTerminalPane = catalogLoader(
  "terminal",
  () => import("$lib/components/terminal/TerminalPane.svelte"),
);
export const loadCalendarPanel = catalogLoader(
  "calendar",
  () => import("$lib/components/calendar/CalendarPanel.svelte"),
);
export const loadMapPanel = catalogLoader(
  "map",
  () => import("$lib/components/context/MapPanel.svelte"),
);
export const loadProfilesPanel = catalogLoader(
  "profiles",
  () => import("$lib/components/profiles/ProfilesPanel.svelte"),
);
export const loadPeersPanel = catalogLoader(
  "peers",
  () => import("$lib/components/peers/PeersPanel.svelte"),
);
export const loadMessagingPanel = catalogLoader(
  "messaging",
  () => import("$lib/components/messaging/MessagingPanel.svelte"),
);
export const loadRuntimePanel = catalogLoader(
  "runtime",
  () => import("$lib/components/runtime/RuntimePanel.svelte"),
);
export const loadEnvironmentRenderer = () =>
  import("$lib/components/environment/EnvironmentRenderer.svelte");

export const loadCommandSpotlight = catalogLoader(
  "spotlight",
  () => import("$lib/components/layout/CommandSpotlight.svelte"),
);
export const loadWizardContainer = catalogLoader(
  "wizard",
  () => import("$lib/components/wizard/WizardContainer.svelte"),
);
export const loadWorkAskDockPopover = () =>
  import("$lib/components/work/WorkAskDockPopover.svelte");
export const loadVaultGarageImportWizard = () =>
  import("$lib/components/vault/VaultGarageImportWizard.svelte");
export const loadVaultContextMenu = () =>
  import("$lib/components/vault/VaultContextMenu.svelte");
export const loadScriptContextMenu = () =>
  import("$lib/components/automations/ScriptContextMenu.svelte");
export const loadShellContextMenu = () =>
  import("$lib/components/shell/ShellContextMenu.svelte");
export const loadVaultAttachmentPanel = () =>
  import("$lib/components/vault/VaultAttachmentPanel.svelte");
export const loadVaultNoteWorkshop = () =>
  import("$lib/components/vault/VaultNoteWorkshop.svelte");
export const loadBrowserWorkshop = () =>
  import("$lib/components/browser/BrowserWorkshop.svelte");
export const loadMobileBrowserWorkshop = () =>
  import("$lib/components/mobile/MobileBrowserWorkshop.svelte");

export const loadLmeEditorHost = catalogLoader(
  "vault-edit",
  () => import("$lib/components/lme/LmeEditorHost.svelte"),
);
export const loadVaultEditor = () => import("$lib/components/vault/VaultEditor.svelte");
export const loadUndertakingsPanel = () =>
  import("$lib/components/work/UndertakingsPanel.svelte");
export const loadCodeSourceEditor = () =>
  import("$lib/components/work/CodeSourceEditor.svelte");
export const loadLmeScriptEditor = () =>
  import("$lib/components/lme/LmeScriptEditor.svelte");
export const loadLmeAgentEditor = () =>
  import("$lib/components/lme/LmeAgentEditor.svelte");
export const loadLmeFlowEditor = () => import("$lib/components/lme/LmeFlowEditor.svelte");
export const loadLmeScheduleEditor = () =>
  import("$lib/components/lme/LmeScheduleEditor.svelte");
export const loadArtifactLibraryPreview = () =>
  import("$lib/components/artifacts/ArtifactLibraryPreview.svelte");
export const loadVaultExportPreviewModal = () =>
  import("$lib/components/vault/VaultExportPreviewModal.svelte");

export const loadSettingsPreferencesSection = () =>
  import("$lib/components/settings/SettingsPreferencesSection.svelte");
export const loadSettingsAgentSection = () =>
  import("$lib/components/settings/SettingsAgentSection.svelte");
export const loadSettingsRuntimeSection = () =>
  import("$lib/components/settings/SettingsRuntimeSection.svelte");
export const loadSettingsNetworkSection = () =>
  import("$lib/components/settings/SettingsNetworkSection.svelte");
export const loadSettingsConnectionsSection = () =>
  import("$lib/components/settings/SettingsConnectionsSection.svelte");
export const loadSettingsPackagesSection = () =>
  import("$lib/components/settings/SettingsPackagesSection.svelte");
export const loadSettingsMcpSection = () =>
  import("$lib/components/settings/SettingsMcpSection.svelte");
export const loadSettingsBasementSection = () =>
  import("$lib/components/settings/SettingsBasementSection.svelte");
