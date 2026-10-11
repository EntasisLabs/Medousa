/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import ChatModelPicker from "./ChatModelPicker.svelte";

const api = vi.hoisted(() => ({ status: vi.fn(), models: vi.fn(), select: vi.fn() }));
vi.mock("$lib/chat/composerModel", () => ({
  composerModel: () => ({ provider: "openai", model: "gpt-6-sol" }),
  selectComposerModel: api.select,
}));
vi.mock("$lib/stores/runtime.svelte", () => ({ runtime: { savingControls: false } }));
vi.mock("$lib/stores/workshopDefaults.svelte", () => ({ workshopDefaults: { loaded: false } }));
vi.mock("$lib/platform", () => ({ isTauriMobilePlatform: () => false }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { isMobile: false, navigateDesktop: vi.fn() } }));
vi.mock("$lib/stores/settingsNav.svelte", () => ({ settingsNav: { openSection: vi.fn() } }));
vi.mock("$lib/daemon", () => ({ getEngineTuiDefaults: async () => ({ favoriteModels: [] }) }));
vi.mock("$lib/utils/providersApi", () => ({
  listProviders: async () => ({ categories: [], providers: [
    { id: "openai", label: "OpenAI", category: "featured", defaultModel: "gpt-6-sol" },
    { id: "openai-codex", label: "ChatGPT account", category: "featured", defaultModel: "gpt-6-sol" },
  ] }),
  probeProviders: async () => null,
  listProviderModels: async () => ({ models: [], source: "provider.models" }),
}));
vi.mock("$lib/utils/modelCapabilityCatalog", async (original) => ({
  ...await original<typeof import("$lib/utils/modelCapabilityCatalog")>(),
  listModelCatalog: async () => ({ models: [] }),
}));
vi.mock("$lib/utils/chatgptOAuth", async (original) => ({
  ...await original<typeof import("$lib/utils/chatgptOAuth")>(),
  getChatGptOAuthConnection: api.status,
  listChatGptOAuthModels: api.models,
}));

let component: ReturnType<typeof mount> | undefined;
const connected = { status: "connected", connected: true, plan_usage_enabled: true };
async function settle() {
  for (let i = 0; i < 8; i++) { flushSync(); await tick(); }
  flushSync();
}
async function openAccount() {
  document.querySelector<HTMLButtonElement>(".composer-model-trigger")!.click();
  await settle();
  document.querySelector<HTMLButtonElement>('[data-provider-id="openai-codex"]')!.click();
  await settle();
}
beforeEach(() => {
  vi.clearAllMocks();
  api.status.mockResolvedValue(connected);
  api.models.mockResolvedValue({ models: ["gpt-6.1-sol"], display_names: { "gpt-6.1-sol": "GPT-6.1 Sol" } });
  HTMLElement.prototype.scrollIntoView = vi.fn();
});
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

it("loads an existing workshop sign-in and selects its account model from an API-key composer", async () => {
  component = mount(ChatModelPicker, { target: document.body });
  await settle();
  expect(api.models).toHaveBeenCalledOnce();
  await openAccount();
  const model = document.querySelector<HTMLButtonElement>('[data-model-key="openai-codex:gpt-6.1-sol"]');
  expect(model?.textContent).toContain("GPT-6.1 Sol");
  model!.click();
  expect(api.select).toHaveBeenCalledWith("openai-codex", "gpt-6.1-sol");
});

it("keeps the ChatGPT provider reachable before its model query finishes", async () => {
  api.models.mockImplementation(() => new Promise(() => {}));
  component = mount(ChatModelPicker, { target: document.body });
  await settle();
  document.querySelector<HTMLButtonElement>(".composer-model-trigger")!.click();
  await settle();
  expect(document.querySelector('[data-model-key="openai:gpt-6-sol"]')).not.toBeNull();
  document.querySelector<HTMLButtonElement>('[data-provider-id="openai-codex"]')!.click();
  await settle();
  expect(document.querySelector('[data-provider-id="openai-codex"]')).not.toBeNull();
  expect(document.querySelector(".composer-model-list")?.textContent).toContain("Checking ChatGPT account");
  expect(api.select).not.toHaveBeenCalled();
});

it("keeps a signed-out account reachable without offering invented subscription models", async () => {
  api.status.mockResolvedValue({ status: "signed_out", connected: false });
  api.models.mockRejectedValue(new Error("ChatGPT account is not connected"));
  component = mount(ChatModelPicker, { target: document.body });
  await settle();
  await openAccount();
  expect(document.querySelector(".composer-model-list")?.textContent).toContain("Connect a ChatGPT account");
  expect(document.querySelector('[data-model-key^="openai-codex:"]')).toBeNull();
});

it("reports catalog failures and lets the connected workshop retry", async () => {
  api.models.mockRejectedValue(new Error("ChatGPT model catalog is unavailable (HTTP 403)"));
  component = mount(ChatModelPicker, { target: document.body });
  await settle();
  await openAccount();
  expect(document.querySelector(".composer-model-list")?.textContent).toContain("HTTP 403");
  const retry = [...document.querySelectorAll("button")].find(button => button.textContent === "Refresh models")!;
  api.models.mockResolvedValue({ models: ["gpt-6.1-sol"], display_names: {} });
  retry.click(); await settle();
  expect(document.querySelector('[data-model-key="openai-codex:gpt-6.1-sol"]')).not.toBeNull();
});

it("does not begin account discovery after an old workshop's picker has unmounted", async () => {
  let finishStatus!: (value: typeof connected) => void;
  api.status.mockImplementation(() => new Promise(resolve => { finishStatus = resolve; }));
  component = mount(ChatModelPicker, { target: document.body });
  await settle();
  await unmount(component); component = undefined;
  finishStatus(connected); await settle();
  expect(api.models).not.toHaveBeenCalled();
});
