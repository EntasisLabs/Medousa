import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

function source(rel: string): string {
  return readFileSync(join(homeRoot, rel), "utf8");
}

describe("former small SCC families stay acyclic", () => {
  it("identity does not import userProfiles", () => {
    expect(source("src/lib/stores/identity.svelte.ts")).not.toMatch(/userProfiles/);
  });

  it("voice presets do not import workshop defaults", () => {
    expect(source("src/lib/stores/voicePresets.svelte.ts")).not.toMatch(/workshopDefaults/);
  });

  it("browser popover overlay does not import the compositor", () => {
    expect(source("src/lib/utils/browserPopoverOverlay.ts")).not.toMatch(/browserCompositor/);
  });

  it("custom vault spaces do not import templates", () => {
    expect(source("src/lib/utils/vaultCustomSpaces.ts")).not.toMatch(/vaultTemplates/);
  });

  it("human browser API does not import the UI store", () => {
    const api = source("src/lib/humanBrowser.ts");
    expect(api).not.toMatch(/humanBrowserSurface/);
    expect(api).not.toMatch(/humanBrowser\.svelte/);
  });

  it("undertakings do not import shellTabs", () => {
    expect(source("src/lib/stores/undertakings.svelte.ts")).not.toMatch(/shellTabs/);
  });

  it("chat and vault do not statically import shellTabs", () => {
    expect(source("src/lib/stores/chat.svelte.ts")).not.toMatch(
      /from ["']\$lib\/stores\/shellTabs/,
    );
    expect(source("src/lib/stores/vault.svelte.ts")).not.toMatch(
      /from ["']\$lib\/stores\/shellTabs/,
    );
  });

  it("shell lifecycle binds feature ports at start", () => {
    const src = source("src/lib/runtime/shellLifecycle.ts");
    expect(src).toContain("bindAllFeaturePorts");
    expect(src).toContain("unbindAllFeaturePorts");
  });
});

describe("former markdown-liquid-vault SCC stays acyclic", () => {
  it("markdown barrel does not re-export hydrate", () => {
    const barrel = source("src/lib/markdown/index.ts");
    expect(barrel).not.toMatch(/hydrateLiquidEmbeds/);
    expect(barrel).not.toMatch(/hydrateMarkdownContainer/);
    expect(barrel).not.toMatch(/hydrateMermaid/);
    expect(barrel).not.toMatch(/hydrateCodeBlocks/);
  });

  it("liquid archetype barrel is descriptors only", () => {
    const barrel = source("src/lib/liquid/archetypes/index.ts");
    expect(barrel).not.toMatch(/registerComponent/);
    expect(barrel).not.toMatch(/liquidOverflow\.css/);
    expect(barrel).not.toMatch(/from ["']\.\/registerUi["']/);
  });

  it("prose does not import MarkdownContent", () => {
    expect(source("src/lib/liquid/archetypes/atoms/prose/Prose.svelte")).not.toMatch(
      /import MarkdownContent/,
    );
  });

  it("vault store does not import the note workshop helper module", () => {
    expect(source("src/lib/stores/vault.svelte.ts")).not.toMatch(/vaultNoteWorkshop/);
  });

  it("workshops store does not import workshopConnection", () => {
    expect(source("src/lib/stores/workshops.svelte.ts")).not.toMatch(/workshopConnection/);
  });

  it("workshop locality does not import the workshops store", () => {
    expect(source("src/lib/utils/workshopLocality.ts")).not.toMatch(/workshops\.svelte/);
  });

  it("chart export does not import vault export hydrate", () => {
    expect(source("src/lib/utils/chartExport.ts")).not.toMatch(/vaultExportPrep/);
  });

  it("Home's interactive stream path stays V3-only", () => {
    const streamPath = [
      "src/lib/stores/chat.svelte.ts",
      "src/lib/chat/streamApplyController.ts",
      "src/lib/stream/eventPump.ts",
      "src/lib/workshopConnection.ts",
      "src/lib/notifications.ts",
      "src/lib/companion/companionState.ts",
      "src/routes/popout/toolbar/+page.svelte",
    ].map(source).join("\n");
    expect(streamPath).not.toMatch(/TurnStreamEnvelopeV2|v2ToLegacy|stream\/v2/);
    expect(streamPath).toMatch(/TurnStreamEnvelopeV3/);
  });

  it("transcript reducer does not import Svelte or the chat store", () => {
    const src = source("src/lib/stream/transcriptReducer.ts");
    expect(src).not.toMatch(/from ["']svelte/);
    expect(src).not.toMatch(/chat\.svelte/);
  });
});
