import { describe, expect, it } from "vitest";
import { applyOps, createScene, findNode } from "$lib/liquid/core";
import { decodeSceneOps } from "$lib/liquid/surfaces/chat/sceneStream";

/**
 * Exercises the pure decode path `EnvironmentSceneView` uses: a durable scene
 * component stores an opaque `config.scene = { ops: [...] }` payload, and the
 * client rebuilds a typed `Scene` from those ops via `decodeSceneOps` + `applyOps`.
 */
function sceneFromConfig(config: Record<string, unknown>, surfaceId: string) {
  const scene = config?.scene as Record<string, unknown> | undefined;
  const rawOps = scene && Array.isArray(scene.ops) ? scene.ops : [];
  const ops = decodeSceneOps(rawOps, surfaceId);
  if (ops.length === 0) return null;
  return applyOps(createScene(surfaceId), ops);
}

describe("environment scene component decode", () => {
  const surface = "env:trip-scene";

  it("builds a renderable scene from an opaque config.scene payload", () => {
    const config = {
      scene: {
        ops: [
          {
            op: "plan_layout",
            surfaceId: "model-chosen",
            rev: 1,
            root: {
              id: "doc",
              type: "document",
              fillState: "ready",
              slots: { flow: [{ id: "p1", type: "prose", props: { markdown: "Hi." }, fillState: "ready" }] },
            },
          },
        ],
      },
    };
    const scene = sceneFromConfig(config, surface);
    expect(scene?.root?.id).toBe("doc");
    // Surface id is client-owned, not whatever the model emitted.
    expect(scene?.root?.type).toBe("document");
    expect(findNode(scene!.root, "p1")?.props.markdown).toBe("Hi.");
  });

  it("returns null for missing or empty scene config", () => {
    expect(sceneFromConfig({}, surface)).toBeNull();
    expect(sceneFromConfig({ scene: {} }, surface)).toBeNull();
    expect(sceneFromConfig({ scene: { ops: [] } }, surface)).toBeNull();
    expect(sceneFromConfig({ scene: { ops: [{ op: "garbage" }] } }, surface)).toBeNull();
  });
});
