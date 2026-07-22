/** `feed` organism — hydrate last-good Stasis/recurring output by feed id. */

import { defineArchetype } from "$lib/liquid/core";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import Feed from "./Feed.svelte";

export const feed = defineArchetype({
  id: "feed",
  tier: "organism",
  props: {
    feedId: { type: "string", required: true },
    datatype: { type: "string", required: true },
    title: { type: "string" },
    empty: { type: "string" },
    refresh: { type: "string" },
  },
  acceptsBindings: ["feed:id"],
  writeCapable: false,
  slots: [],
  emits: [],
  virtualization: "none",
  defaultOwner: "agent",
});

registerComponent(feed.id, Feed);
