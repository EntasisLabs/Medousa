/** Optional packages offered during first-run connections beat. */
export const WIZARD_PACKAGE_OFFER_IDS = [
  "adapter-discord",
  "adapter-telegram",
  "adapter-whatsapp",
  "mcp-gateway",
] as const;

export type WizardPackageOfferId = (typeof WIZARD_PACKAGE_OFFER_IDS)[number];

export const WIZARD_PACKAGE_OFFER_FALLBACK: Array<{
  id: WizardPackageOfferId;
  displayName: string;
  hint: string;
}> = [
  { id: "adapter-discord", displayName: "Discord", hint: "Answer from your Discord" },
  { id: "adapter-telegram", displayName: "Telegram", hint: "Answer from Telegram" },
  { id: "adapter-whatsapp", displayName: "WhatsApp", hint: "Answer from WhatsApp" },
  { id: "mcp-gateway", displayName: "MCP gateway", hint: "Bring outside tools into Medousa" },
];
