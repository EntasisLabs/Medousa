/** `whisper` atom — descriptor only. */

import { defineArchetype } from "$lib/liquid/core";

export const whisper = defineArchetype({
  id: "whisper",
  tier: "atom",
  props: { text: { type: "string", required: true } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});
