/** `media` atom — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Media from "./Media.svelte";

export const media = defineArchetype({
  id: "media",
  tier: "atom",
  props: {
    src: { type: "string", required: true },
    alt: { type: "string" },
    caption: { type: "string" },
    ratio: { type: "string" },
  },
  acceptsBindings: ["inline", "artifact:id"],
  writeCapable: false,
  slots: [],
  emits: ["navigate"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(media.id, Media);
