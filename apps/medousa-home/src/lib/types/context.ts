export type ContextTabId = "recall" | "threads" | "posture" | "map";

export type ContextRecallKind =
  | "claim"
  | "contact"
  | "relationship"
  | "persona"
  | "user";

export interface ContextRecallEntry {
  id: string;
  kind: ContextRecallKind;
  title: string;
  subtitle: string;
  searchText: string;
  confidence?: number;
  trustLevel?: number;
  meta?: Record<string, string>;
}

export const CONTEXT_TABS: {
  id: ContextTabId;
  label: string;
  hint: string;
  available: boolean;
}[] = [
  {
    id: "recall",
    label: "Recall",
    hint: "Facts and people she remembers",
    available: true,
  },
  {
    id: "threads",
    label: "Threads",
    hint: "Moments from your sessions",
    available: true,
  },
  {
    id: "posture",
    label: "Posture",
    hint: "How you showed up",
    available: true,
  },
  {
    id: "map",
    label: "Map",
    hint: "See how sessions connect",
    available: true,
  },
];
