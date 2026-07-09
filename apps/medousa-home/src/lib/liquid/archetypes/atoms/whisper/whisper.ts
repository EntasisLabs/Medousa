/** `whisper` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Whisper from "./Whisper.svelte";

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

registerComponent(whisper.id, Whisper);
