/** `chat_media` shell archetype — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import ChatMedia from "./ChatMedia.svelte";

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

registerComponent(chatMedia.id, ChatMedia);
