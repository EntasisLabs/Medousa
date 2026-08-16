/** `chat_media` shell archetype — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const chatMedia = defineArchetype({
  id: "chat_media",
  tier: "shell",
  props: { attachments: { type: "array", required: true } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

