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
    expect(html).toContain("--medousa-host-accent");
    expect(html).toContain("--medousa-host-brand");
    expect(html).not.toContain("!important");
  });

  it("injects scrollable panel mode styles without layout overrides", () => {
    const authoredStyle = "body{color:#242424;background:#faf9f6;margin:40px auto;padding:0 24px}";
    const html = prepareArtifactHtml(
      `<!DOCTYPE html><html><head><style>${authoredStyle}</style><style id="medousa-artifact-host">html,body{margin:0;padding:0;background:transparent;overflow:hidden}</style></head><body><p>Hi</p></body></html>`,
      "panel",
      false,
    );
    expect(html).toContain("overflow:auto");
    expect(html).toContain("--medousa-host-fg:#18181b");
    expect(html).toContain(authoredStyle);
    expect(html).not.toContain('id="medousa-artifact-host"');
    expect(buildArtifactModeStyle("panel")).not.toContain("background:");
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
    expect(html).toContain("medousa-feed-client-script");
  });

  it("injects runtime bridge for presentation widgets", () => {
    const html = prepareArtifactHtml(
      "<!DOCTYPE html><html><head></head><body></body></html>",
      "panel",
      true,
      null,
      "braindump-widget",
    );
    expect(html).toContain("medousa-artifact-runtime-script");
    expect(html).toContain("medousa:artifact:runtime");
    expect(html).toContain("medousa-store-bootstrap-script");
  });
});
