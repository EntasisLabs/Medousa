/** `code` molecule — enhanced snippet with lang badge + copy (```code fences). */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Code from "./Code.svelte";

export const code = defineArchetype({
  id: "code",
  tier: "molecule",
  props: {
    source: { type: "string", required: true },
    lang: { type: "string" },
    title: { type: "string" },
    diff: { type: "boolean" },
    copy: { type: "boolean" },
  },
  acceptsBindings: ["inline"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(code.id, Code);
