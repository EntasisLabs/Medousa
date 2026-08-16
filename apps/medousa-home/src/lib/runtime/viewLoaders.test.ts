import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

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
    const host = readFileSync(
      join(homeRoot, "src/lib/components/lme/LmeEditorHost.svelte"),
      "utf8",
    );
    expect(host).not.toMatch(/import VaultEditor from/);
    expect(host).not.toMatch(/import UndertakingsPanel from/);
    expect(host).toContain("loadVaultEditor");
  });
});
