export type SettingsSectionId =
  | "room"
  | "rhythm"
  | "memory"
  | "voice"
  | "reach"
  | "phone"
  | "basement";

export const SETTINGS_SECTIONS: {
  id: SettingsSectionId;
  label: string;
  hint: string;
}[] = [
  { id: "room", label: "Room", hint: "Theme & atmosphere" },
  { id: "rhythm", label: "Rhythm", hint: "How she interrupts you" },
  { id: "memory", label: "Memory", hint: "What stays vivid" },
  { id: "voice", label: "Voice", hint: "How she answers" },
  { id: "reach", label: "Reach", hint: "What she may touch" },
  { id: "phone", label: "Phone", hint: "Pair your pocket portal" },
  { id: "basement", label: "Basement", hint: "Connection & power tools" },
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

export const STAGE_ROLE_LABELS: Record<string, string> = {
  orchestrator: "Lead",
  chunker: "Reader",
  extractor: "Extractor",
  summarizer: "Summarizer",
  verifier: "Verifier",
  packer: "Packer",
  final_response: "Final voice",
};
