import { Plus, ImagePlus, Workflow, PanelsTopLeft, Image } from "@lucide/svelte";
export const creativeStarters = [
  { id: "blank", label: "Blank canvas", icon: Plus },
  { id: "screenshot", label: "Screenshot markup", icon: ImagePlus },
  { id: "diagram", label: "Diagram", icon: Workflow },
  { id: "concept", label: "UI concept", icon: PanelsTopLeft },
  { id: "composition", label: "Image composition", icon: Image },
] as const;
