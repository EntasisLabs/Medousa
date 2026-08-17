import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import { disposeFeature, loadedFeature, resetFeaturesForTests } from "./features/loader";
import { loadCatalogView } from "./viewLoaders";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

afterEach(async () => {
  await disposeFeature("vault-browse", "teardown");
  resetFeaturesForTests();
});

describe("ShellPane destination loaders", () => {
  it("statically imports chat chrome and not dormant destination panels", () => {
    const source = readFileSync(
      join(homeRoot, "src/lib/components/shell/ShellPane.svelte"),
      "utf8",
    );
    expect(source).toMatch(/import ChatSessionView from/);
    expect(source).toMatch(/import ChatPaneIdle from/);
    expect(source).not.toMatch(/import LmePanel from/);
    expect(source).not.toMatch(/import WorkPanel from/);
    expect(source).not.toMatch(/import HumanBrowserPanel from/);
    expect(source).not.toMatch(/import SettingsPanel from/);
    expect(source).not.toMatch(/import TerminalPane from/);
    expect(source).not.toMatch(/import CalendarPanel from/);
    expect(source).toContain("loadLmePanel");
    expect(source).toContain("EmptyState");
    expect(source).toContain("openEmptyPaneSurface");
    expect(source).not.toMatch(/Open something from the rail\./);
    expect(source).not.toMatch(/void import\(/);
    expect(source).not.toMatch(/onMount\(/);
  });
});

describe("browse vs edit and settings splits", () => {
  it("does not statically import the vault editor from the browse pane", () => {
    const pane = readFileSync(
      join(homeRoot, "src/lib/components/lme/LmePanel.svelte"),
      "utf8",
    );
    expect(pane).not.toMatch(/import LmeEditorHost from/);
    expect(pane).not.toMatch(/@tiptap/);
    expect(pane).not.toMatch(/@codemirror/);
    expect(pane).toContain("loadLmeEditorHost");
    expect(pane).toContain("VaultEmptyState");
    expect(pane).toContain("EmptyState");
    const host = readFileSync(
      join(homeRoot, "src/lib/components/lme/LmeEditorHost.svelte"),
      "utf8",
    );
    expect(host).not.toMatch(/import VaultEditor from/);
    expect(host).not.toMatch(/import UndertakingsPanel from/);
    expect(host).toContain("loadVaultEditor");
  });

  it("does not evaluate the source editor from the work hub", () => {
    const work = readFileSync(
      join(homeRoot, "src/lib/components/work/WorkPanel.svelte"),
      "utf8",
    );
    expect(work).not.toMatch(/CodeSourceEditor/);
    expect(work).not.toMatch(/@codemirror/);
    const undertakings = readFileSync(
      join(homeRoot, "src/lib/components/work/UndertakingsPanel.svelte"),
      "utf8",
    );
    expect(undertakings).not.toMatch(/import CodeSourceEditor from/);
    expect(undertakings).toContain("loadCodeSourceEditor");
  });

  it("does not import settings packages or agent subsections from the settings root", () => {
    const source = readFileSync(
      join(homeRoot, "src/lib/components/layout/SettingsPanel.svelte"),
      "utf8",
    );
    expect(source).not.toMatch(/import SettingsPackagesSection from/);
    expect(source).not.toMatch(/import SettingsAgentSection from/);
    expect(source).not.toMatch(/import SettingsPreferencesSection from/);
    expect(source).toContain("loadSettingsPackagesSection");
  });
});

describe("vault and code destinations are feature instances", () => {
  it("loads browse, edit, attachments, and import/export through the catalog", () => {
    const source = readFileSync(join(homeRoot, "src/lib/runtime/viewLoaders.ts"), "utf8");
    expect(source).toContain('catalogLoader(\n  "vault-browse"');
    expect(source).toContain('catalogLoader(\n  "vault-edit"');
    expect(source).toContain('catalogLoader(\n  "export-import"');
    expect(source).toContain('catalogLoader(\n  "code-work"');
    expect(source).toContain("loadVaultAttachmentPanel");
    expect(source).toContain("loadVaultGarageImportWizard");
    expect(source).toContain("loadVaultExportPreviewModal");
    expect(source).toContain("loadCodeSourceEditor");
    expect(source).toContain("loadUndertakingsPanel");
  });
});

describe("catalog view loader lifecycle", () => {
  it("loads different views of one feature concurrently", async () => {
    const [panel, editor] = await Promise.all([
      loadCatalogView("vault-browse", "panel", async () => ({ default: "panel" })),
      loadCatalogView("vault-browse", "editor", async () => ({ default: "editor" })),
    ]);

    expect(panel.default).toBe("panel");
    expect(editor.default).toBe("editor");
    panel.release();
    editor.release();
    await vi.waitFor(() => expect(loadedFeature("vault-browse")).toBeUndefined());
  });

  it("does not publish a view after its feature is disposed", async () => {
    let releaseImport: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseImport = resolve;
    });
    const pending = loadCatalogView("vault-browse", "panel", async () => {
      await gate;
      return { default: "late-panel" };
    });

    await vi.waitFor(() => expect(loadedFeature("vault-browse")).toBeDefined());
    await disposeFeature("vault-browse", "navigate-away");
    releaseImport();

    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(loadedFeature("vault-browse")).toBeUndefined();
  });

  it("lets one cancelled waiter leave a shared view import", async () => {
    const bootstrap = await loadCatalogView(
      "vault-browse",
      "bootstrap",
      async () => ({ default: "bootstrap" }),
    );
    let releaseImport: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseImport = resolve;
    });
    const importer = async () => {
      await gate;
      return { default: "panel" };
    };
    const cancelled = new AbortController();
    const first = loadCatalogView("vault-browse", "panel", importer, cancelled.signal);
    const second = loadCatalogView("vault-browse", "panel", importer);

    cancelled.abort("navigation");
    releaseImport();

    await expect(first).rejects.toMatchObject({ name: "AbortError" });
    const live = await second;
    expect(live.default).toBe("panel");
    live.release();
    bootstrap.release();
  });
});
