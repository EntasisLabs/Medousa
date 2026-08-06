import { describe, expect, it } from "vitest";
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import {
  applyHomeOnboardingEnvironment,
  onboardingPackageIds,
  onboardingSurfaceOrder,
  onboardingWorkspaceSurfaces,
  runHomeOnboardingTasks,
} from "$lib/utils/homeOnboarding";

describe("Home onboarding", () => {
  it("does not block completion when persistence must sync later", async () => {
    const completed: string[] = [];
    const deferred = await runHomeOnboardingTasks([
      { label: "Home name", run: async () => { completed.push("name"); } },
      { label: "Home layout", run: async () => { throw new Error("daemon warming"); } },
      { label: "Theme", run: async () => { completed.push("theme"); } },
    ]);

    expect(completed).toEqual(["name", "theme"]);
    expect(deferred).toEqual(["Home layout"]);
  });

  it("maps coding and selected channels to optional packages only", () => {
    expect(onboardingPackageIds(["code", "messaging"], ["discord", "slack"])).toEqual([
      "coding-engine",
      "langservers",
      "shell-session",
      "adapter-discord",
      "adapter-slack",
    ]);
    expect(onboardingPackageIds(["notes", "plan", "ai"], [])).toEqual([]);
  });

  it("orders chosen destinations once and keeps safety surfaces", () => {
    expect(onboardingSurfaceOrder(["messaging", "ai", "notes"])).toEqual([
      "home",
      "messaging",
      "chat",
      "map",
      "notes",
      "files",
      "artifacts",
      "web",
      "settings",
      "runtime",
    ]);
    expect(onboardingWorkspaceSurfaces(["messaging", "ai"])).toEqual([
      "messaging",
      "chat",
      "map",
    ]);
  });

  it("applies the chosen shell shape to the active layout", () => {
    const spec = defaultEnvironmentSpec();
    applyHomeOnboardingEnvironment(spec, ["code", "notes"], "focused");
    const active = spec.layoutPresets?.find((preset) => preset.active);
    expect(active?.surfaces).toEqual([
      "home",
      "code",
      "notes",
      "files",
      "artifacts",
      "web",
      "settings",
      "runtime",
    ]);
    expect(active?.shellChrome?.desktop).toMatchObject({
      navStyle: "compact",
      activityRail: "hidden",
      vaultSidebar: "hidden",
    });
  });
});
