import { describe, expect, it } from "vitest";

import { mapStreamUiArtifact, replaceUiArtifactEntry } from "$lib/types/artifact";

describe("artifact chat helpers", () => {
  it("maps stream ui artifact presentation", () => {
    const mapped = mapStreamUiArtifact(
      {
        artifact_id: "art:1:ui:abc",
        mime: "text/html",
        label: "Day Recap",
        presentation: "panel",
        byte_size: 100,
        height_px: 480,
      },
      "art:1:ui:root",
    );
    expect(mapped.artifactId).toBe("art:1:ui:abc");
    expect(mapped.presentation).toBe("panel");
    expect(mapped.rootArtifactId).toBe("art:1:ui:root");
  });

  it("replaces artifact by previous id", () => {
    const existing = [
      {
        artifactId: "art:old",
        mime: "text/html",
        label: "Old",
        presentation: "inline" as const,
        byteSize: 1,
        heightPx: null,
        rootArtifactId: "art:root",
      },
    ];
    const next = {
      artifactId: "art:new",
      mime: "text/html",
      label: "New",
      presentation: "inline" as const,
      byteSize: 2,
      heightPx: null,
      rootArtifactId: "art:root",
    };
    const updated = replaceUiArtifactEntry(existing, "art:old", "art:root", next);
    expect(updated).toHaveLength(1);
    expect(updated[0]?.artifactId).toBe("art:new");
    expect(updated[0]?.rootArtifactId).toBe("art:root");
  });
});
