import { describe, expect, it } from "vitest";

import {
  buildArtifactModeStyle,
  buildArtifactThemeStyle,
  prepareArtifactHtml,
} from "$lib/utils/artifactPrepareHtml";

describe("artifactPrepareHtml", () => {
  it("injects theme and inline mode styles", () => {
    const html = prepareArtifactHtml("<div>Chart</div>", "inline", true);
    expect(html).toContain("medousa-artifact-theme");
    expect(html).toContain("medousa-artifact-mode");
    expect(html).toContain("overflow:hidden");
    expect(html).toContain("--medousa-host-fg");
    expect(html).not.toContain("!important");
  });

  it("injects scrollable panel mode styles without layout overrides", () => {
    const html = prepareArtifactHtml(
      "<!DOCTYPE html><html><head></head><body><p>Hi</p></body></html>",
      "panel",
      false,
    );
    expect(html).toContain("overflow:auto");
    expect(html).toContain("--medousa-host-fg:#18181b");
    expect(html).not.toContain("justify-content");
    expect(html).not.toContain("body>*");
  });

  it("does not duplicate style blocks", () => {
    const once = prepareArtifactHtml("<html><head></head><body></body></html>", "fullscreen", true);
    const twice = prepareArtifactHtml(once, "fullscreen", true);
    expect(twice.match(/medousa-artifact-theme/g)?.length).toBe(1);
    expect(twice.match(/medousa-artifact-mode/g)?.length).toBe(1);
  });

  it("builds mode style helpers", () => {
    expect(buildArtifactThemeStyle(false)).toContain("medousa-artifact-theme");
    expect(buildArtifactModeStyle("fullscreen")).toContain("overflow:auto");
    expect(buildArtifactModeStyle("inline")).toContain("overflow:hidden");
  });

  it("injects workshop feed state for live presentation components", () => {
    const html = prepareArtifactHtml(
      "<!DOCTYPE html><html><head></head><body></body></html>",
      "panel",
      true,
      {
        feedId: "workshop.pulse",
        lastPatch: { phase: "working", round: 2, tools: ["cognition_environment_apply"] },
      },
    );
    expect(html).toContain("window.__MEDOUSA_FEED__=");
    expect(html).toContain('"phase":"working"');
    expect(html).toContain("workshop.pulse");
  });
});
