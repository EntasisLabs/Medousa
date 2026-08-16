/** Ports so LME owns tab list/focus and does not import sibling feature stores. */

import type { LibrarySidebarMode } from "$lib/types/externalDesk";
import type { FlowComposerDraft } from "$lib/types/workflow";

export type LmeCodeBuffer = {
  tabId: string;
  path: string;
  title: string;
  line: number | null;
  loading: boolean;
};

export type LmeScriptTab = {
  tabId: string;
  scriptId: string | null;
  name: string;
};

export type LmeWorkspacePorts = {
  selectedNotePath: () => string | null;
  labelForPath: (path: string) => string;
  openNote: (path: string, options?: { skipLeaveFlush?: boolean }) => Promise<void>;
  flushBeforeLeave: () => Promise<boolean>;
  previewAttachment: (path: string, presentation: "pane" | "panel") => void;
  previewingAttachmentPath: () => string | null;
  closeAttachmentPreview: () => void;
  setSidebarMode: (mode: LibrarySidebarMode) => void;
  selectExternalPath: (path: string) => void;
  openScriptById: (scriptId: string) => Promise<void>;
  openNewScriptTab: () => void;
  scriptTabs: () => LmeScriptTab[];
  activeScriptTab: () => LmeScriptTab | null;
  selectScriptTab: (tabId: string) => void;
  closeScriptTab: (tabId: string) => void;
  openCodeBuffer: (
    workId: string,
    path: string,
    line: number | null,
    options?: { recordNavigation?: boolean },
  ) => Promise<{ title?: string } | null>;
  codeTabsFor: (workId: string) => LmeCodeBuffer[];
  hydrateCode: (workId: string) => Promise<void>;
  closeCodeBuffer: (tabId: string) => void;
  isCodeDirty: (workId: string, buffer: LmeCodeBuffer) => boolean;
  selectUndertaking: (workId: string) => Promise<void>;
  undertakingDetailId: () => string | null;
  setUndertakingSelection: (selection: {
    path: string;
    line: number | null;
    entityId: string | null;
  }) => void;
  artifactLabel: (artifactId: string) => string | undefined;
  selectArtifact: (artifactId: string) => void;
  loadManuscriptDetail: (manuscriptId: string) => void;
  composerOpen: () => boolean;
  setComposerOpen: (open: boolean) => void;
  composerDraftName: () => string;
  openComposer: (seed?: Partial<FlowComposerDraft>) => void;
  closeComposer: () => void;
  loadFlowDetail: (workflowId: string) => void;
  loadFlowRuns: (workflowId: string) => void;
  loadAutomationRuns: (recurringId: string) => void;
};

const unbound: LmeWorkspacePorts = {
  selectedNotePath: () => null,
  labelForPath: (path) => path.split("/").pop() ?? path,
  openNote: async () => {},
  flushBeforeLeave: async () => true,
  previewAttachment: () => {},
  previewingAttachmentPath: () => null,
  closeAttachmentPreview: () => {},
  setSidebarMode: () => {},
  selectExternalPath: () => {},
  openScriptById: async () => {},
  openNewScriptTab: () => {},
  scriptTabs: () => [],
  activeScriptTab: () => null,
  selectScriptTab: () => {},
  closeScriptTab: () => {},
  openCodeBuffer: async () => null,
  codeTabsFor: () => [],
  hydrateCode: async () => {},
  closeCodeBuffer: () => {},
  isCodeDirty: () => false,
  selectUndertaking: async () => {},
  undertakingDetailId: () => null,
  setUndertakingSelection: () => {},
  artifactLabel: () => undefined,
  selectArtifact: () => {},
  loadManuscriptDetail: () => {},
  composerOpen: () => false,
  setComposerOpen: () => {},
  composerDraftName: () => "",
  openComposer: () => {},
  closeComposer: () => {},
  loadFlowDetail: () => {},
  loadFlowRuns: () => {},
  loadAutomationRuns: () => {},
};

let ports: LmeWorkspacePorts | null = null;

export function setLmeWorkspacePorts(next: LmeWorkspacePorts | null): void {
  ports = next;
}

export function lmeWorkspacePorts(): LmeWorkspacePorts {
  return ports ?? unbound;
}
