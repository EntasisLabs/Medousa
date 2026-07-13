export type SettingsSectionId =
  | "room"
  | "canvas"
  | "rhythm"
  | "memory"
  | "models"
  | "voice"
  | "reach"
  | "shell"
  | "engine"
  | "phone"
  | "nearby"
  | "packages"
  | "basement";

export const SETTINGS_SECTIONS: {
  id: SettingsSectionId;
  label: string;
  hint: string;
}[] = [
  { id: "room", label: "Room", hint: "Theme & atmosphere" },
  { id: "canvas", label: "Canvas", hint: "Layout presets & agent proposals" },
  { id: "rhythm", label: "Rhythm", hint: "How she interrupts you" },
  { id: "memory", label: "Memory", hint: "How long chats stay vivid" },
  { id: "models", label: "Models", hint: "Chat & dictation" },
  { id: "voice", label: "Voice", hint: "Stance & answer depth" },
  { id: "reach", label: "Reach", hint: "What she may touch" },
  { id: "shell", label: "Shell", hint: "Process sandbox & agent tools" },
  { id: "engine", label: "Engine", hint: "Budgets, quality & diagnostics" },
  { id: "phone", label: "Phone", hint: "Pair your pocket portal" },
  { id: "nearby", label: "Nearby", hint: "Peers rail, bundles & trust" },
  { id: "packages", label: "Packages", hint: "Offline brain, adapters & tools" },
  { id: "basement", label: "Connection", hint: "This device, engine & advanced files" },
];

export const DEPTH_CHARTER_OPTIONS = [
  {
    id: "concise" as const,
    label: "Concise",
    hint: "Short answers — less reasoning on the page",
  },
  {
    id: "standard" as const,
    label: "Standard",
    hint: "Balanced depth for everyday work",
  },
  {
    id: "deep" as const,
    label: "Deep",
    hint: "More thorough reasoning and detail",
  },
];

export const TOOL_CALL_CHARTER_OPTIONS = [
  {
    id: "auto" as const,
    label: "Flexible",
    hint: "She decides when tools are worth calling",
  },
  {
    id: "strict" as const,
    label: "Careful",
    hint: "Tighter rules before invoking tools",
  },
] as const;

export const HOST_BUS_CHARTER_OPTIONS = [
  {
    id: "auto" as const,
    label: "When needed",
    hint: "Bring specialists in only when the turn needs help",
  },
  {
    id: "force" as const,
    label: "Always",
    hint: "Route through the specialist bus every turn",
  },
  {
    id: "off" as const,
    label: "Direct",
    hint: "Orchestrator only — no specialist bus",
  },
] as const;
