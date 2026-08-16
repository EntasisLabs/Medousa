import type { FeatureId, FeatureInstance } from "./features/types";
import { disposeFeature, loadFeature } from "./features/loader";
import { probeClientPlatform } from "./platformProbe";

export type FeatureViewModule<T = unknown> = { default: T };

export type FeatureViewLease<T = unknown> = FeatureViewModule<T> & {
  release(): void;
};

type ViewLoadRecord = {
  promise: Promise<FeatureViewModule>;
  abort: AbortController;
  waiters: number;
};

type MultiViewInstance = FeatureInstance & {
  views: Map<string, FeatureViewModule>;
  setFeatureCleanup(cleanup: () => void): void;
  loadView<T>(
    viewKey: string,
    importer: () => Promise<FeatureViewModule<T>>,
    signal?: AbortSignal,
  ): Promise<FeatureViewLease<T>>;
};

function cancelled(id: FeatureId): Error {
  return new DOMException(`feature ${id} view load cancelled`, "AbortError");
}

function createMultiViewInstance(id: FeatureId): MultiViewInstance {
  const views = new Map<string, FeatureViewModule>();
  const inflightViews = new Map<string, ViewLoadRecord>();
  let disposed = false;
  let leases = 0;
  let stopFeature = () => {};

  const waitForView = <T>(
    viewKey: string,
    record: ViewLoadRecord,
    signal?: AbortSignal,
  ): Promise<FeatureViewLease<T>> => {
    if (signal?.aborted) return Promise.reject(cancelled(id));
    record.waiters += 1;
    return new Promise((resolve, reject) => {
      let finished = false;
      const finish = () => {
        if (finished) return false;
        finished = true;
        signal?.removeEventListener("abort", onAbort);
        record.waiters -= 1;
        return true;
      };
      const onAbort = () => {
        if (!finish()) return;
        if (record.waiters === 0 && inflightViews.get(viewKey) === record) {
          record.abort.abort(signal?.reason);
        }
        reject(cancelled(id));
      };
      signal?.addEventListener("abort", onAbort, { once: true });
      record.promise.then(
        (view) => {
          if (!finish()) return;
          leases += 1;
          let released = false;
          resolve({
            ...(view as FeatureViewModule<T>),
            release() {
              if (released) return;
              released = true;
              leases -= 1;
              queueMicrotask(() => {
                if (leases === 0 && inflightViews.size === 0 && !disposed) {
                  void disposeFeature(id, "navigate-away");
                }
              });
            },
          });
        },
        (error) => {
          if (finish()) reject(error);
        },
      );
    });
  };

  const instance: MultiViewInstance = {
    views,
    setFeatureCleanup(cleanup) {
      if (disposed) cleanup();
      else stopFeature = cleanup;
    },
    async loadView<T>(
      viewKey: string,
      importer: () => Promise<FeatureViewModule<T>>,
      signal?: AbortSignal,
    ) {
      if (disposed || signal?.aborted) throw cancelled(id);
      const cached = views.get(viewKey);
      if (cached) {
        const resolved: ViewLoadRecord = {
          promise: Promise.resolve(cached),
          abort: new AbortController(),
          waiters: 0,
        };
        return waitForView<T>(viewKey, resolved, signal);
      }
      let record = inflightViews.get(viewKey);
      if (!record) {
        const abort = new AbortController();
        let created: ViewLoadRecord;
        const promise: Promise<FeatureViewModule> = importer().then((view) => {
          if (disposed || abort.signal.aborted) throw cancelled(id);
          views.set(viewKey, view);
          return view;
        }).finally(() => {
          if (inflightViews.get(viewKey) === created) inflightViews.delete(viewKey);
        });
        created = { promise, abort, waiters: 0 };
        record = created;
        inflightViews.set(viewKey, created);
      }
      return waitForView<T>(viewKey, record, signal);
    },
    dispose() {
      disposed = true;
      for (const record of inflightViews.values()) record.abort.abort("navigate-away");
      inflightViews.clear();
      views.clear();
      stopFeature();
      stopFeature = () => {};
    },
  };
  return instance;
}

async function startFeatureResources(
  id: FeatureId,
  signal: AbortSignal,
): Promise<() => void> {
  if (id !== "browser") return () => {};
  const { attachAgentBrowserCoord } = await import("$lib/utils/agentBrowserCoord");
  if (signal.aborted) return () => {};
  return attachAgentBrowserCoord();
}

export async function loadCatalogView<T>(
  id: FeatureId,
  viewKey: string,
  importer: () => Promise<FeatureViewModule<T>>,
  signal?: AbortSignal,
): Promise<FeatureViewLease<T>> {
  const instance = await loadFeature(
    id,
    async () => ({
      async start(context) {
        const instance = createMultiViewInstance(id);
        context.track(instance);
        instance.setFeatureCleanup(await startFeatureResources(id, context.signal));
        return instance;
      },
    }),
    { platform: probeClientPlatform(), signal },
  );
  return (instance as MultiViewInstance).loadView(viewKey, importer, signal);
}

function catalogLoader<T>(
  id: FeatureId,
  viewKey: string,
  importer: () => Promise<FeatureViewModule<T>>,
): (signal?: AbortSignal) => Promise<FeatureViewLease<T>> {
  return (signal) => loadCatalogView(id, viewKey, importer, signal);
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
