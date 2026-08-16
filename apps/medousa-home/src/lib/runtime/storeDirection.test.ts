import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function source(rel: string): string {
  return readFileSync(join(homeRoot, rel), "utf8");
}

describe("sibling feature stores do not import each other", () => {
  it("artifacts does not import chat", () => {
    expect(source("src/lib/stores/artifacts.svelte.ts")).not.toMatch(/chat\.svelte/);
  });

  it("userProfiles does not import chat or identity", () => {
    const src = source("src/lib/stores/userProfiles.svelte.ts");
    expect(src).not.toMatch(/chat\.svelte/);
    expect(src).not.toMatch(/identity\.svelte/);
  });

  it("workshops does not import chat, vault, or settings", () => {
    const src = source("src/lib/stores/workshops.svelte.ts");
    expect(src).not.toMatch(/chat\.svelte/);
    expect(src).not.toMatch(/vault\.svelte/);
    expect(src).not.toMatch(/settings\.svelte/);
  });

  it("workspace does not import chat or settings", () => {
    const src = source("src/lib/stores/workspace.svelte.ts");
    expect(src).not.toMatch(/chat\.svelte/);
    expect(src).not.toMatch(/settings\.svelte/);
  });

  it("chat does not import settings", () => {
    expect(source("src/lib/stores/chat.svelte.ts")).not.toMatch(/settings\.svelte/);
  });

  it("workshopDefaults does not import runtime or voicePresets", () => {
    const src = source("src/lib/stores/workshopDefaults.svelte.ts");
    expect(src).not.toMatch(/runtime\.svelte/);
    expect(src).not.toMatch(/voicePresets\.svelte/);
  });

  it("settings, settingsNav, vaultVersions, and workshops do not import workshopDefaults", () => {
    for (const rel of [
      "src/lib/stores/settings.svelte.ts",
      "src/lib/stores/settingsNav.svelte.ts",
      "src/lib/stores/vaultVersions.svelte.ts",
      "src/lib/stores/workshops.svelte.ts",
    ]) {
      expect(source(rel)).not.toMatch(/workshopDefaults\.svelte/);
    }
  });

  it("sharedMode does not import userProfiles", () => {
    expect(source("src/lib/stores/sharedMode.svelte.ts")).not.toMatch(/userProfiles\.svelte/);
  });

  it("spotlightPins does not import workshops", () => {
    expect(source("src/lib/stores/spotlightPins.svelte.ts")).not.toMatch(/workshops\.svelte/);
  });

  it("lmeWorkspace does not import sibling feature stores", () => {
    const src = source("src/lib/stores/lmeWorkspace.svelte.ts");
    for (const name of [
      "artifacts.svelte",
      "automations.svelte",
      "catalog.svelte",
      "codeWorkspace.svelte",
      "externalDesk.svelte",
      "flows.svelte",
      "graphemeScriptEditor.svelte",
      "vault.svelte",
      "undertakings.svelte",
      "shellTabs.svelte",
    ]) {
      expect(src).not.toMatch(new RegExp(`from ["']\\$lib/stores/${name}`));
    }
  });

  it("shellTabs does not import sibling feature stores", () => {
    const src = source("src/lib/stores/shellTabs.svelte.ts");
    for (const name of [
      "chat.svelte",
      "codeWorkspace.svelte",
      "humanBrowser.svelte",
      "vault.svelte",
    ]) {
      expect(src).not.toMatch(new RegExp(`from ["']\\$lib/stores/${name}`));
    }
    expect(src).not.toMatch(/import \{[^}]*lmeWorkspace[^}]*\} from ["']\$lib\/stores\/lmeWorkspace/);
  });
});

describe("code/work panels consume controllers", () => {
  it("CodeSourceEditor does not import forge directly", () => {
    expect(source("src/lib/components/work/CodeSourceEditor.svelte")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/components/work/CodeSourceEditor.svelte")).toContain(
      "codeDocumentService",
    );
  });

  it("H09 Code editor extracts do not import forge directly", () => {
    const files = [
      "src/lib/code/codeChangesController.svelte.ts",
      "src/lib/code/codeTasksController.svelte.ts",
      "src/lib/code/codeProblemsController.svelte.ts",
      "src/lib/code/codeQuickOpenController.svelte.ts",
      "src/lib/code/codeSaveController.svelte.ts",
      "src/lib/code/codeEditorWindowKeys.ts",
      "src/lib/components/code/CodeEditorChrome.svelte",
      "src/lib/components/code/CodeEditorWorkspace.svelte",
      "src/lib/components/code/CodeEditorDialogs.svelte",
      "src/lib/components/code/CodeContextSidePanel.svelte",
      "src/lib/components/code/CodeTasksOutput.svelte",
      "src/lib/components/code/CodeQuickOpenModal.svelte",
    ];
    for (const rel of files) {
      expect(source(rel), rel).not.toMatch(/from ["']\$lib\/forge["']/);
    }
  });

  it("H09 chat extracts do not import the chat store or forge", () => {
    const files = [
      "src/lib/chat/sessionController.ts",
      "src/lib/chat/streamLifecycleController.ts",
      "src/lib/chat/streamApplyController.ts",
      "src/lib/chat/workerLaneController.ts",
      "src/lib/chat/turnSideEffectsAdapter.ts",
      "src/lib/chat/mediaAttachController.ts",
      "src/lib/chat/chatStoreHost.ts",
      "src/lib/stream/transcriptReducer.ts",
    ];
    for (const rel of files) {
      const src = source(rel);
      expect(src, rel).not.toMatch(/from ["']\$lib\/stores\/chat/);
      expect(src, rel).not.toMatch(/from ["']\$lib\/forge["']/);
    }
  });

  it("UndertakingsPanel does not import forge directly", () => {
    expect(source("src/lib/components/work/UndertakingsPanel.svelte")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/components/work/UndertakingsPanel.svelte")).toContain(
      "undertakingCommandController",
    );
  });

  it("UndertakingWorldPanel and world controller do not import forge directly", () => {
    expect(source("src/lib/components/work/UndertakingWorldPanel.svelte")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/components/work/UndertakingWorldPanel.svelte")).not.toMatch(
      /CodeSourceEditor/,
    );
    expect(source("src/lib/work/undertakingWorldController.ts")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/work/undertakingWorldController.ts")).toContain(
      "undertakingCommandController",
    );
  });

  it("UndertakingReviewCanvas does not import forge directly", () => {
    expect(source("src/lib/components/work/UndertakingReviewCanvas.svelte")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/components/work/UndertakingReviewCanvas.svelte")).toContain(
      "undertakingCommandController",
    );
    expect(source("src/lib/components/work/UndertakingReviewCanvas.svelte")).not.toMatch(
      /CodeSourceEditor/,
    );
  });
});

describe("vault lookup injection and destination dispose", () => {
  it("vault store publishes the H07 lookup snapshot", () => {
    expect(source("src/lib/stores/vault.svelte.ts")).toContain("publishVaultLookupSnapshot");
    expect(source("src/lib/stores/vault.svelte.ts")).toContain("setVaultNoteBufferPort");
  });

  it("H09 vault extracts do not import VaultEditor components", () => {
    const files = [
      "src/lib/vault/vaultBrowseController.ts",
      "src/lib/vault/vaultEditorController.ts",
      "src/lib/vault/vaultBridgeController.ts",
      "src/lib/vault/vaultRootsController.ts",
      "src/lib/vault/vaultRailController.ts",
    ];
    for (const rel of files) {
      expect(source(rel), rel).not.toMatch(/VaultEditor\.svelte|VaultLiveEditor/);
    }
  });

  it("workshop switch and platform switch dispose destination features", () => {
    expect(source("src/lib/stores/shellTabs.svelte.ts")).toContain(
      'disposeDestinationFeatures("workshop-switch")',
    );
    expect(source("src/lib/runtime/shellLifecycle.ts")).toContain(
      'disposeDestinationFeatures("platform-switch")',
    );
    expect(source("src/lib/runtime/shellLifecycle.ts")).toContain(
      'disposeDestinationFeatures("teardown")',
    );
  });
});
