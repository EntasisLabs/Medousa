import { describe, expect, it } from "vitest";
import type { ComponentDef } from "$lib/types/environment";
import {
  presentationBare,
  presentationEmbedMode,
  surfaceUsesDashboardFill,
} from "./environmentPresentation";

const presentationComponent = (
  overrides: Partial<ComponentDef> = {},
): ComponentDef => ({
  id: "demo",
  type: "presentation",
  surfaceId: "adhd-guide",
  slot: "main",
  label: "Demo",
  config: { artifactId: "art:demo" },
  presentation: "inline",
  feeds: [],
  ...overrides,
});

describe("environmentPresentation", () => {
  it("uses panel fill for dashboard main slot even when component says inline", () => {
    expect(
      presentationEmbedMode(
        "dashboard",
        presentationComponent({ presentation: "inline" }),
      ),
    ).toBe("panel");
  });

  it("keeps inline mode for non-dashboard surfaces", () => {
    expect(
      presentationEmbedMode("single", presentationComponent({ presentation: "inline" })),
    ).toBe("inline");
  });

  it("marks dashboard presentations as bare chrome", () => {
    expect(presentationBare("dashboard", "panel")).toBe(true);
    expect(presentationBare("single", "inline")).toBe(false);
  });

  it("detects dashboard fill surfaces", () => {
    expect(surfaceUsesDashboardFill("dashboard")).toBe(true);
    expect(surfaceUsesDashboardFill("single")).toBe(false);
  });
});
