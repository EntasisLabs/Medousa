import { beforeEach, describe, expect, it, vi } from "vitest";

const preview = vi.hoisted(() => ({
  local: false,
  getDaemonUrl: vi.fn(async () => "https://workshop.example/"),
  create: vi.fn(async () => ({ preview_path: "/v1/forge/preview/fresh/" })),
}));

vi.mock("$lib/daemon", () => ({ getDaemonUrl: preview.getDaemonUrl }));
vi.mock("$lib/utils/workshopLocality", () => ({
  isCoLocatedWorkshop: () => preview.local,
}));
vi.mock("$lib/forge", () => ({ createProjectTaskRunPreview: preview.create }));

import {
  isHttpDaemonBase,
  resolveTaskPreviewOpenUrl,
} from "$lib/code/taskPreviewUrl";

describe("task preview url", () => {
  beforeEach(() => {
    preview.local = false;
    preview.getDaemonUrl.mockClear();
    preview.create.mockClear();
  });

  it("accepts http workshop bases for proxy handoff", () => {
    expect(isHttpDaemonBase("http://192.168.1.10:7420")).toBe(true);
    expect(isHttpDaemonBase("https://workshop.example")).toBe(true);
    expect(isHttpDaemonBase("iroh://ticket")).toBe(false);
    expect(isHttpDaemonBase("")).toBe(false);
  });

  it("opens a co-located ready service directly", async () => {
    preview.local = true;
    await expect(resolveTaskPreviewOpenUrl("work-1", {
      run_id: "run-1",
      work_id: "work-1",
      state: "ready",
      task: {
        id: "dev",
        label: "Dev",
        kind: "run",
        provider: "npm",
        argv: ["npm", "run", "dev"],
      },
      ready_url: "http://127.0.0.1:5173",
    })).resolves.toEqual({ url: "http://127.0.0.1:5173", via: "direct" });
    expect(preview.create).not.toHaveBeenCalled();
  });

  it("reattaches a remote preview from its retained authorized path", async () => {
    await expect(resolveTaskPreviewOpenUrl("work-1", {
      run_id: "run-1",
      work_id: "work-1",
      state: "ready",
      task: {
        id: "dev",
        label: "Dev",
        kind: "run",
        provider: "npm",
        argv: ["npm", "run", "dev"],
      },
      ready_url: "http://127.0.0.1:5173",
      preview_path: "/v1/forge/preview/retained/",
    })).resolves.toEqual({
      url: "https://workshop.example/v1/forge/preview/retained/",
      via: "proxy",
    });
    expect(preview.create).not.toHaveBeenCalled();
  });
});
