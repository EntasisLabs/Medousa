/** `carousel` molecule — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Carousel from "./Carousel.svelte";

export const carousel = defineArchetype({
  id: "carousel",
  tier: "molecule",
  props: {},
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["items"],
  emits: ["select", "scroll_end"],
  virtualization: "window",
  defaultOwner: "agent",
});

registerComponent(carousel.id, Carousel);
