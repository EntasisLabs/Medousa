export type SettingsSectionId =
  | "general"
  | "appearance"
  | "connection"
  | "integrations"
  | "advanced";

export const SETTINGS_SECTIONS: {
  id: SettingsSectionId;
  label: string;
  hint: string;
}[] = [
  { id: "general", label: "General", hint: "Notifications & activity" },
  { id: "appearance", label: "Appearance", hint: "Theme & dark mode" },
  { id: "connection", label: "Connection", hint: "Workshop link" },
  { id: "integrations", label: "Integrations", hint: "Channels & schedules" },
  { id: "advanced", label: "Advanced", hint: "Files & diagnostics" },
];
