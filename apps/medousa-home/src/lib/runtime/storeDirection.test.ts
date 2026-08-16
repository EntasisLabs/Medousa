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

  it("UndertakingsPanel does not import forge directly", () => {
    expect(source("src/lib/components/work/UndertakingsPanel.svelte")).not.toMatch(
      /from ["']\$lib\/forge["']/,
    );
    expect(source("src/lib/components/work/UndertakingsPanel.svelte")).toContain(
      "undertakingCommandController",
    );
  });
});

describe("vault lookup injection and destination dispose", () => {
  it("vault store publishes the H07 lookup snapshot", () => {
    expect(source("src/lib/stores/vault.svelte.ts")).toContain("publishVaultLookupSnapshot");
    expect(source("src/lib/stores/vault.svelte.ts")).toContain("setVaultNoteBufferPort");
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
