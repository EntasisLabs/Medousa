/**
 * Merge a daemon-authored Liquid body into the runtime turn ceremony.
 *
 * Builder tools (`cognition_ui_build`) emit a body-only document. Ceremony
 * (thinking, live pulse, tools) stays client-owned. Legacy freeform `ui_scene`
 * roots that are not builder bodies still take over the whole turn.
 */

import type { SceneNode } from "$lib/liquid/core";
import type { ChatMessage } from "$lib/types/chat";
import { chatMessageToScene, type ChatSceneOptions } from "./messageToScene";

/** True when the daemon root is a builder body document (not a full freeform turn). */
export function isBuilderBodyScene(root: SceneNode): boolean {
  if (root.id.startsWith("build:")) return true;
  const flow = root.slots?.flow ?? [];
  return (
    root.type === "document" &&
    flow.some((node) => node.type === "stack" && node.id.includes(":body"))
  );
}

/** Children that should paint as the turn's substance body. */
export function extractBuilderBodyChildren(root: SceneNode): SceneNode[] {
  const flow = root.slots?.flow ?? [];
  const stack = flow.find((node) => node.type === "stack") ?? flow[0];
  if (!stack) return [];
  if (stack.type === "stack") {
    return stack.slots?.children ?? [];
  }
  return flow;
}

/**
 * Prefer ceremony template + builder body children. Falls back to the daemon
 * root when it does not look like a builder body (legacy freeform scenes).
 */
export function resolveChatScene(
  message: ChatMessage,
  opts: ChatSceneOptions,
  daemonRoot: SceneNode | null | undefined,
): SceneNode {
  if (!daemonRoot) {
    return chatMessageToScene(message, opts);
  }
  if (!isBuilderBodyScene(daemonRoot)) {
    return daemonRoot;
  }

  const bodyChildren = extractBuilderBodyChildren(daemonRoot);
  if (bodyChildren.length === 0) {
    return chatMessageToScene(message, opts);
  }

  const ceremony = chatMessageToScene(message, opts);
  const flow = [...(ceremony.slots?.flow ?? [])];
  const withoutProseBody = flow.filter((node) => node.id !== `${message.id}:body`);

  // Insert builder body after thinking / pulse / errors — before tools.
  let insertAt = withoutProseBody.findIndex((node) => {
    const id = node.id;
    return (
      id === `${message.id}:tools` ||
      id === `${message.id}:artifacts` ||
      id.endsWith(":tools") ||
      id.endsWith(":artifacts")
    );
  });
  if (insertAt < 0) {
    insertAt = withoutProseBody.length;
  }

  withoutProseBody.splice(insertAt, 0, ...bodyChildren);
  return {
    ...ceremony,
    slots: { flow: withoutProseBody },
  };
}
