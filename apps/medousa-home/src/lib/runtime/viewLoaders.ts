import type { FeatureId, FeatureInstance } from "./features/types";
import { loadFeature, loadedFeature } from "./features/loader";
import { probeClientPlatform } from "./platformProbe";

export type FeatureViewModule<T = unknown> = { default: T };

type MultiViewInstance = FeatureInstance & {
  views: Map<string, FeatureViewModule>;
};

async function loadCatalogView<T>(
  id: FeatureId,
  viewKey: string,
  importer: () => Promise<FeatureViewModule<T>>,
): Promise<FeatureViewModule<T>> {
  const existing = loadedFeature(id) as MultiViewInstance | undefined;
  if (existing?.views.has(viewKey)) {
    return existing.views.get(viewKey)! as FeatureViewModule<T>;
  }
  if (existing) {
    const view = await importer();
    existing.views.set(viewKey, view);
    return view;
  }
  await loadFeature(
    id,
    async () => ({
      async start() {
        const view = await importer();
        const views = new Map<string, FeatureViewModule>([[viewKey, view]]);
        return {
          views,
          dispose() {
            views.clear();
          },
        } satisfies MultiViewInstance;
      },
    }),
    { platform: probeClientPlatform() },
  );
  const loaded = loadedFeature(id) as MultiViewInstance | undefined;
  const view = loaded?.views.get(viewKey) as FeatureViewModule<T> | undefined;
  if (!view) throw new Error(`feature ${id} produced no view ${viewKey}`);
  return view;
}

function catalogLoader<T>(
  id: FeatureId,
  viewKey: string,
  importer: () => Promise<FeatureViewModule<T>>,
): () => Promise<FeatureViewModule<T>> {
  return () => loadCatalogView(id, viewKey, importer);
}

export const loadLmePanel = catalogLoader(
  "vault-browse",
  "panel",
  () => import("$lib/components/lme/LmePanel.svelte"),
);
export const loadWorkPanel = catalogLoader(
  "code-work",
  "panel",
  () => import("$lib/components/work/WorkPanel.svelte"),
);
export const loadHumanBrowserPanel = catalogLoader(
  "browser",
  "panel",
  () => import("$lib/components/browser/HumanBrowserPanel.svelte"),
);
export const loadSettingsPanel = catalogLoader(
  "settings",
  "panel",
  () => import("$lib/components/layout/SettingsPanel.svelte"),
);
export const loadTerminalPane = catalogLoader(
  "terminal",
  "pane",
  () => import("$lib/components/terminal/TerminalPane.svelte"),
);
export const loadCalendarPanel = catalogLoader(
  "calendar",
  "panel",
  () => import("$lib/components/calendar/CalendarPanel.svelte"),
);
export const loadMapPanel = catalogLoader(
  "map",
  "panel",
  () => import("$lib/components/context/MapPanel.svelte"),
);
export const loadProfilesPanel = catalogLoader(
  "profiles",
  "panel",
  () => import("$lib/components/profiles/ProfilesPanel.svelte"),
);
export const loadPeersPanel = catalogLoader(
  "peers",
  "panel",
  () => import("$lib/components/peers/PeersPanel.svelte"),
);
export const loadMessagingPanel = catalogLoader(
  "messaging",
  "panel",
  () => import("$lib/components/messaging/MessagingPanel.svelte"),
);
export const loadRuntimePanel = catalogLoader(
  "runtime",
  "panel",
  () => import("$lib/components/runtime/RuntimePanel.svelte"),
);
export const loadEnvironmentRenderer = () =>
  import("$lib/components/environment/EnvironmentRenderer.svelte");

export const loadCommandSpotlight = catalogLoader(
  "spotlight",
  "view",
  () => import("$lib/components/layout/CommandSpotlight.svelte"),
);
export const loadWizardContainer = catalogLoader(
  "wizard",
  "view",
  () => import("$lib/components/wizard/WizardContainer.svelte"),
);
export const loadWorkAskDockPopover = () =>
  import("$lib/components/work/WorkAskDockPopover.svelte");
export const loadVaultGarageImportWizard = catalogLoader(
  "export-import",
  "import",
  () => import("$lib/components/vault/VaultGarageImportWizard.svelte"),
);
export const loadVaultContextMenu = () =>
  import("$lib/components/vault/VaultContextMenu.svelte");
export const loadScriptContextMenu = () =>
  import("$lib/components/automations/ScriptContextMenu.svelte");
export const loadShellContextMenu = () =>
  import("$lib/components/shell/ShellContextMenu.svelte");
export const loadVaultAttachmentPanel = catalogLoader(
  "export-import",
  "attachments",
  () => import("$lib/components/vault/VaultAttachmentPanel.svelte"),
);
export const loadVaultNoteWorkshop = catalogLoader(
  "vault-edit",
  "workshop",
  () => import("$lib/components/vault/VaultNoteWorkshop.svelte"),
);
export const loadBrowserWorkshop = () =>
  import("$lib/components/browser/BrowserWorkshop.svelte");
export const loadMobileBrowserWorkshop = () =>
  import("$lib/components/mobile/MobileBrowserWorkshop.svelte");

export const loadLmeEditorHost = catalogLoader(
  "vault-edit",
  "host",
  () => import("$lib/components/lme/LmeEditorHost.svelte"),
);
export const loadVaultEditor = catalogLoader(
  "vault-edit",
  "editor",
  () => import("$lib/components/vault/VaultEditor.svelte"),
);
export const loadUndertakingsPanel = catalogLoader(
  "code-work",
  "undertakings",
  () => import("$lib/components/work/UndertakingsPanel.svelte"),
);
export const loadCodeSourceEditor = catalogLoader(
  "code-work",
  "editor",
  () => import("$lib/components/work/CodeSourceEditor.svelte"),
);
export const loadLmeScriptEditor = () =>
  import("$lib/components/lme/LmeScriptEditor.svelte");
export const loadLmeAgentEditor = () =>
  import("$lib/components/lme/LmeAgentEditor.svelte");
export const loadLmeFlowEditor = () => import("$lib/components/lme/LmeFlowEditor.svelte");
export const loadLmeScheduleEditor = () =>
  import("$lib/components/lme/LmeScheduleEditor.svelte");
export const loadArtifactLibraryPreview = () =>
  import("$lib/components/artifacts/ArtifactLibraryPreview.svelte");
export const loadVaultExportPreviewModal = catalogLoader(
  "export-import",
  "export",
  () => import("$lib/components/vault/VaultExportPreviewModal.svelte"),
);

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
