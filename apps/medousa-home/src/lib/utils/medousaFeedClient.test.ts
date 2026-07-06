import { describe, expect, it } from "vitest";

import { prepareArtifactHtml } from "$lib/utils/artifactPrepareHtml";
import {
  buildMedousaFeedClientScript,
  isMedousaFeedTailRequest,
  isValidFeedId,
  MEDOUSA_FEED_CLIENT_SCRIPT_ID,
} from "$lib/utils/medousaFeedClient";

describe("medousaFeedClient", () => {
  it("builds injectable client script with custom element", () => {
    const script = buildMedousaFeedClientScript();
    expect(script).toContain(`id="${MEDOUSA_FEED_CLIENT_SCRIPT_ID}"`);
    expect(script).toContain("customElements.define");
    expect(script).toContain("medousa-feed");
    expect(script).toContain("MedousaFeed");
  });

  it("validates feed ids like the daemon", () => {
    expect(isValidFeedId("trip.london.trains")).toBe(true);
    expect(isValidFeedId("Bad-ID")).toBe(false);
  });

  it("recognizes tail request postMessages", () => {
    expect(
      isMedousaFeedTailRequest({
        type: "medousa:feed:tail",
        requestId: "mf-1",
        feedId: "trip.london.trains",
      }),
    ).toBe(true);
    expect(isMedousaFeedTailRequest({ type: "other" })).toBe(false);
  });
});

describe("artifactPrepareHtml feed client", () => {
  it("injects medousa feed client script and styles", () => {
    const html = prepareArtifactHtml("<div>Live board</div>", "panel", true);
    expect(html).toContain(MEDOUSA_FEED_CLIENT_SCRIPT_ID);
    expect(html).toContain("medousa-feed-card");
    expect(html).toContain("MedousaFeed");
  });
});
