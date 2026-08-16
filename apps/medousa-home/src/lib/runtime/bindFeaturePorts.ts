/** Composition-root binders. Feature stores must not import this module. */

import { persistTuiRuntimePrefs } from "$lib/config";
import { artifacts } from "$lib/stores/artifacts.svelte";
import { automations } from "$lib/stores/automations.svelte";
import { catalog } from "$lib/stores/catalog.svelte";
import { chat } from "$lib/stores/chat.svelte";
import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
import { externalDesk } from "$lib/stores/externalDesk.svelte";
import { flows } from "$lib/stores/flows.svelte";
import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settings } from "$lib/stores/settings.svelte";
import { undertakings } from "$lib/stores/undertakings.svelte";
import { userProfiles } from "$lib/stores/userProfiles.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { voicePresets } from "$lib/stores/voicePresets.svelte";
import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
import { workshops } from "$lib/stores/workshops.svelte";
import { normalizeReasoningEffort } from "$lib/types/reasoningEffort";
import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
import { setChatSettingsPort } from "./chatSettingsPort";
import { setLmeWorkspacePorts } from "./lmeWorkspacePorts";
import { setSharedModePort } from "./sharedModePorts";
import { setShellTabPorts } from "./shellTabPorts";
import {
  setWorkshopDefaultsQueryPort,
  setWorkshopDefaultsSyncPort,
} from "./workshopDefaultsPorts";

export function bindAllFeaturePorts(): void {
  setChatSettingsPort({
    autoOpenWebOnAgentBrowse: () => settings.autoOpenWebOnAgentBrowse,
    showEngineDetailsInChat: () => settings.showEngineDetailsInChat,
  });

  setWorkshopDefaultsSyncPort({
    applyVoiceDraft: (draft) => voicePresets.applyFromDraft(draft),
    applyRuntimeFromDefaults: async (payload) => {
      runtime.provider = payload.provider ?? runtime.provider;
      runtime.model = payload.model ?? runtime.model;
      if (
        payload.responseDepthMode === "concise" ||
        payload.responseDepthMode === "standard" ||
        payload.responseDepthMode === "deep"
      ) {
        runtime.depthMode = payload.responseDepthMode;
      }
      if (payload.reasoningEffort) {
        runtime.reasoningEffort = normalizeReasoningEffort(payload.reasoningEffort);
      }
      if (payload.stageRouting) {
        runtime.stageRouting = payload.stageRouting;
      }
      voicePresets.applyFromDraft(payload);
      await persistTuiRuntimePrefs(
        runtime.provider,
        runtime.model,
        runtime.depthMode,
        runtime.reasoningEffort,
        payload.stageRouting ?? undefined,
      );
      runtime.defaultsLoaded = true;
    },
  });

  setWorkshopDefaultsQueryPort({
    loaded: () => workshopDefaults.loaded,
    dirty: () => workshopDefaults.dirty,
    workCardHideAfterHours: () => workshopDefaults.draft.workCardHideAfterHours,
    workCardWipeAfterDays: () => workshopDefaults.draft.workCardWipeAfterDays,
    vaultGitEnabled: () => workshopDefaults.draft.vaultGitEnabled,
    setVaultGitEnabled: (enabled) => {
      workshopDefaults.draft = {
        ...workshopDefaults.draft,
        vaultGitEnabled: enabled,
      };
    },
    save: () => workshopDefaults.save(),
    resetForReconnect: () => workshopDefaults.resetForReconnect(),
    load: (force) => workshopDefaults.load(force),
  });

  setSharedModePort({
    reloadUserProfiles: () => userProfiles.load({ suppressRemoteNotice: true }),
  });

  setActiveWorkshopIdPort(() => workshops.activeWorkshopId);

  setLmeWorkspacePorts({
    selectedNotePath: () => vault.selectedPath,
    labelForPath: (path) => vault.labelByPathMap.get(path) ?? path.split("/").pop() ?? path,
    openNote: (path, options) => vault.openNote(path, options),
    flushBeforeLeave: () => vault.flushBeforeLeave(),
    previewAttachment: (path, presentation) => vault.previewAttachment(path, presentation),
    previewingAttachmentPath: () => vault.previewingAttachmentPath,
    closeAttachmentPreview: () => vault.closeAttachmentPreview(),
    setSidebarMode: (mode) => externalDesk.setSidebarMode(mode),
    selectExternalPath: (path) => externalDesk.selectExternalPath(path),
    openScriptById: (scriptId) => graphemeScriptEditor.openScriptById(scriptId),
    openNewScriptTab: () => graphemeScriptEditor.openNewTab(),
    scriptTabs: () => graphemeScriptEditor.tabs,
    activeScriptTab: () => graphemeScriptEditor.activeTab,
    selectScriptTab: (tabId) => graphemeScriptEditor.selectTab(tabId),
    closeScriptTab: (tabId) => graphemeScriptEditor.closeTab(tabId),
    openCodeBuffer: async (workId, path, line, options) => {
      const source = await codeWorkspace.open(workId, path, line, options);
      return source ? { title: source.title } : null;
    },
    codeTabsFor: (workId) =>
      codeWorkspace.tabsFor(workId).map((tab) => ({
        tabId: tab.tabId,
        path: tab.path,
        title: tab.title,
        line: tab.line,
        loading: tab.loading,
      })),
    hydrateCode: (workId) => codeWorkspace.hydrate(workId),
    closeCodeBuffer: (tabId) => codeWorkspace.close(tabId),
    isCodeDirty: (workId, buffer) => {
      const match = codeWorkspace.tabsFor(workId).find((entry) => entry.tabId === buffer.tabId);
      return match ? codeWorkspace.isDirty(match) : false;
    },
    selectUndertaking: (workId) => undertakings.select(workId),
    undertakingDetailId: () => undertakings.detail?.id ?? null,
    setUndertakingSelection: (selection) => undertakings.setSelection(selection),
    artifactLabel: (artifactId) =>
      artifacts.artifacts.find((row) => row.artifact_id === artifactId)?.label,
    selectArtifact: (artifactId) => artifacts.selectArtifact(artifactId),
    loadManuscriptDetail: (manuscriptId) => {
      void catalog.loadManuscriptDetail(manuscriptId);
    },
    composerOpen: () => flows.composerOpen,
    setComposerOpen: (open) => {
      flows.composerOpen = open;
    },
    composerDraftName: () => flows.composerDraft.name,
    openComposer: (seed) => flows.openComposer(seed),
    closeComposer: () => flows.closeComposer(),
    loadFlowDetail: (workflowId) => {
      void flows.loadDetail(workflowId);
    },
    loadFlowRuns: (workflowId) => {
      void flows.loadRuns(workflowId);
    },
    loadAutomationRuns: (recurringId) => {
      void automations.loadRuns(recurringId);
    },
  });

  setShellTabPorts({
    chat: {
      sessionId: () => chat.sessionId,
      sessions: () => chat.sessions,
      messagesFor: (sessionId) => chat.messagesFor(sessionId),
      historyLoadingFor: (sessionId) => chat.historyLoadingFor(sessionId),
      warmBackgroundSession: (sessionId) => {
        void chat.warmBackgroundSession(sessionId);
      },
      switchSession: (sessionId) => chat.switchSession(sessionId),
      newSession: (options) => {
        void chat.newSession(options);
      },
    },
    lme: {
      tabs: () => lmeWorkspace.tabs,
      activeTab: () => lmeWorkspace.activeTab,
      activeTabId: () => lmeWorkspace.activeTabId,
      captureSession: () => lmeWorkspace.captureSession(),
      restoreSession: (value) => lmeWorkspace.restoreSession(value),
      activateTab: (tabId) => lmeWorkspace.activateTab(tabId),
      closeTab: (tabId, options) => lmeWorkspace.closeTab(tabId, options),
      confirmCloseTab: (tabId) => lmeWorkspace.confirmCloseTab(tabId),
    },
    vault: {
      flushBeforeLeave: () => vault.flushBeforeLeave(),
      openNote: (path) => vault.openNote(path),
      isFocusedPath: (path) => vault.isFocusedPath(path),
    },
    browser: {
      tabs: () => humanBrowser.tabs,
      activeTab: () => humanBrowser.activeTab,
      activateTab: (tabId) => humanBrowser.activateTab(tabId),
      closeTab: (tabId) => {
        void humanBrowser.closeTab(tabId);
      },
      openTab: async (url) => {
        await humanBrowser.openTab(url);
      },
    },
    code: {
      resetForWorkshopSwitch: () => codeWorkspace.resetForWorkshopSwitch(),
    },
  });
}

export function unbindAllFeaturePorts(): void {
  setChatSettingsPort(null);
  setWorkshopDefaultsSyncPort(null);
  setWorkshopDefaultsQueryPort(null);
  setSharedModePort(null);
  setActiveWorkshopIdPort(null);
  setLmeWorkspacePorts(null);
  setShellTabPorts(null);
}
