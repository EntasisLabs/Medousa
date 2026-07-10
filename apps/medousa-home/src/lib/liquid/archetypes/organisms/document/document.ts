/** `document` organism — descriptor + self-registration. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Document from "./Document.svelte";

export const document = defineArchetype({
  id: "document",
  tier: "organism",
  props: { scroll: { type: "string" } },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: ["flow"],
  emits: ["navigate", "pin", "scroll_end"],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(document.id, Document);
