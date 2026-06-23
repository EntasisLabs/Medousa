<script lang="ts">
  import { onMount } from "svelte";
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import SettingsInferenceProfile from "$lib/components/settings/SettingsInferenceProfile.svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import {
    syncFlatFieldsFromProfiles,
    visionProfileReady,
    type InferenceProfiles,
  } from "$lib/types/inferenceProfiles";
  import { defaultSttModel } from "$lib/types/workshopDefaults";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { formatModelDisplayName } from "$lib/utils/formatModelDisplay";
  import { providerMonogram } from "$lib/utils/chatModelPicker";
  import { favoriteToPick } from "$lib/utils/modelCatalog";
  import { listProviders, probeProviders, type ProvidersListResult } from "$lib/utils/providersApi";
  import { composerSttStatus } from "$lib/utils/composerStt";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let catalog = $state<ProvidersListResult | null>(null);
  let ollamaDetected = $state(false);
  let sttReady = $state(false);
  let sttReason = $state<string | null>(null);
  let providerStatus = $state<string | null>(null);
  let sttProviderStatus = $state<string | null>(null);
  let visionProviderStatus = $state<string | null>(null);

  const visionProfile = $derived(workshopDefaults.draft.inferenceProfiles?.vision ?? null);
  const visionStatusOk = $derived(visionProfileReady(workshopDefaults.draft.inferenceProfiles));

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const chatQuickIds = $derived(
    ollamaDetected
      ? ["deepseek", "openai", "anthropic", "ollama"]
      : ["deepseek", "openai", "anthropic", "groq"],
  );

  const chatStatusOk = $derived(
    Boolean(workshopDefaults.draft.provider?.trim()) &&
      Boolean(workshopDefaults.draft.model?.trim()) &&
      (workshopDefaults.apiKeySet ||
        workshopDefaults.draft.provider?.trim().toLowerCase() === "ollama"),
  );

  const chatStatusLabel = $derived(chatStatusOk ? "Ready" : "Needs setup");
  const chatStatusDetail = $derived(
    chatStatusOk
      ? `${formatModelDisplayName(workshopDefaults.draft.model ?? "")} powers chat turns.`
      : "Choose a provider and add an API key — or pick Ollama for local chat.",
  );

  const sttStatusLabel = $derived(sttReady ? "Ready" : "Needs setup");

  onMount(() => {
    void bootstrap();
  });

  async function bootstrap() {
    try {
      const [listed, probe, stt] = await Promise.all([
        listProviders(),
        probeProviders(),
        composerSttStatus(),
      ]);
      catalog = listed;
      ollamaDetected = probe.ollamaDetected;
      sttReady = stt.available;
      sttReason = stt.reason;
    } catch {
      catalog = null;
    }
  }

  function mergeProfiles(patch: Partial<InferenceProfiles>) {
    return {
      main: workshopDefaults.draft.inferenceProfiles?.main ?? null,
      vision: workshopDefaults.draft.inferenceProfiles?.vision ?? null,
      stt: workshopDefaults.draft.inferenceProfiles?.stt ?? null,
      ...patch,
    };
  }

  function onChatProviderChange(id: string, entry: ProviderCatalogEntry) {
    workshopDefaults.draft = syncFlatFieldsFromProfiles({
      ...workshopDefaults.draft,
      provider: id,
      model: entry.defaultModel,
      baseUrl: entry.defaultBaseUrl,
      inferenceProfiles: mergeProfiles({
        main: {
          provider: id,
          model: entry.defaultModel,
          baseUrl: entry.defaultBaseUrl,
          fallbacks: workshopDefaults.draft.inferenceProfiles?.main?.fallbacks ?? [],
        },
      }),
    });
    providerStatus = null;
  }

  function onVisionProviderChange(id: string, entry: ProviderCatalogEntry) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      inferenceProfiles: mergeProfiles({
        vision: {
          provider: id,
          model: entry.defaultModel,
          baseUrl: entry.defaultBaseUrl,
          fallbacks: workshopDefaults.draft.inferenceProfiles?.vision?.fallbacks ?? [],
        },
      }),
    };
    visionProviderStatus = null;
  }

  function onSttProviderChange(id: string, entry: ProviderCatalogEntry) {
    workshopDefaults.draft = syncFlatFieldsFromProfiles({
      ...workshopDefaults.draft,
      sttProvider: id,
      sttModel: defaultSttModel(id),
      sttBaseUrl: entry.defaultBaseUrl,
      inferenceProfiles: mergeProfiles({
        stt: {
          provider: id,
          model: defaultSttModel(id),
          baseUrl: entry.defaultBaseUrl,
          fallbacks: workshopDefaults.draft.inferenceProfiles?.stt?.fallbacks ?? [],
        },
      }),
    });
    sttProviderStatus = null;
    void refreshSttStatus();
  }

  const favorites = $derived(workshopDefaults.favoriteModels());

  function applyFavorite(provider: string, model: string) {
    workshopDefaults.draft = syncFlatFieldsFromProfiles({
      ...workshopDefaults.draft,
      provider,
      model,
      inferenceProfiles: mergeProfiles({
        main: {
          provider,
          model,
          baseUrl: workshopDefaults.draft.baseUrl ?? null,
          fallbacks: workshopDefaults.draft.inferenceProfiles?.main?.fallbacks ?? [],
        },
      }),
    });
    providerStatus = null;
  }

  async function refreshSttStatus() {
    const stt = await composerSttStatus();
    sttReady = stt.available;
    sttReason = stt.reason;
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Models</h2>
    <p class="workshop-faint mt-1 text-sm">
      Who powers chat — and who transcribes the mic. Independent choices; save once at the bottom.
    </p>
  </header>

  <div class="settings-profile-stack mt-5">
    <SettingsInferenceProfile
      title="Chat model"
      subtitle="The mind behind every turn in the composer."
      {catalog}
      providerId={workshopDefaults.draft.provider ?? "deepseek"}
      model={workshopDefaults.draft.model ?? ""}
      baseUrl={workshopDefaults.draft.baseUrl ?? ""}
      apiKey={workshopDefaults.apiKeyDraft}
      apiKeySet={workshopDefaults.apiKeySet}
      quickProviderIds={chatQuickIds}
      excludeProviderIds={["medousa-local"]}
      statusOk={chatStatusOk}
      statusLabel={chatStatusLabel}
      statusDetail={chatStatusDetail}
      showSuggestedModels
      showFavoriteToggle
      favoriteModels={favorites}
      onToggleFavorite={(provider, model) => workshopDefaults.toggleFavorite(provider, model)}
      disabled={readOnly || workshopDefaults.saving}
      onProviderChange={onChatProviderChange}
      onModelChange={(value) =>
        (workshopDefaults.draft = syncFlatFieldsFromProfiles({
          ...workshopDefaults.draft,
          model: value,
          inferenceProfiles: mergeProfiles({
            main: {
              provider: workshopDefaults.draft.provider ?? "deepseek",
              model: value,
              baseUrl: workshopDefaults.draft.baseUrl ?? null,
              fallbacks: workshopDefaults.draft.inferenceProfiles?.main?.fallbacks ?? [],
            },
          }),
        }))}
      onApiKeyChange={(value) => (workshopDefaults.apiKeyDraft = value)}
      onBaseUrlChange={(value) =>
        (workshopDefaults.draft = syncFlatFieldsFromProfiles({
          ...workshopDefaults.draft,
          baseUrl: value,
          inferenceProfiles: mergeProfiles({
            main: {
              provider: workshopDefaults.draft.provider ?? "deepseek",
              model: workshopDefaults.draft.model ?? "",
              baseUrl: value,
              fallbacks: workshopDefaults.draft.inferenceProfiles?.main?.fallbacks ?? [],
            },
          }),
        }))}
      onStatus={(message, ok) => {
        providerStatus = message;
        if (ok === true) providerStatus = message;
      }}
    />

    {#if favorites.length > 0}
      <article class="settings-profile-card">
        <header class="settings-profile-header">
          <div class="min-w-0">
            <h3 class="settings-profile-title">Favorites</h3>
            <p class="settings-profile-subtitle">
              Pinned for one-tap access in the composer model menu.
            </p>
          </div>
        </header>
        <ul class="settings-favorites-list">
          {#each favorites as entry (entry.provider + entry.model)}
            {@const pick = favoriteToPick(entry)}
            {@const active =
              workshopDefaults.draft.provider === entry.provider &&
              workshopDefaults.draft.model === entry.model}
            <li class="settings-favorites-row">
              <button
                type="button"
                class="settings-favorites-main {active ? 'is-active' : ''}"
                disabled={readOnly || workshopDefaults.saving}
                onclick={() => applyFavorite(entry.provider, entry.model)}
              >
                <span class="settings-profile-badge" aria-hidden="true">
                  {providerMonogram(entry.provider)}
                </span>
                <span class="min-w-0 flex-1 text-left">
                  <span class="settings-profile-model">{pick.label}</span>
                  <span class="settings-profile-provider">{pick.hint ?? entry.provider}</span>
                </span>
              </button>
              <button
                type="button"
                class="settings-favorites-remove"
                disabled={readOnly || workshopDefaults.saving}
                title="Remove favorite"
                onclick={() => void workshopDefaults.toggleFavorite(entry.provider, entry.model)}
              >
                Remove
              </button>
            </li>
          {/each}
        </ul>
      </article>
    {/if}

    <SettingsInferenceProfile
      title="Vision model"
      subtitle="Required for image attachments — separate from chat model."
      {catalog}
      providerId={visionProfile?.provider ?? "openai"}
      model={visionProfile?.model ?? "gpt-4o-mini"}
      baseUrl={visionProfile?.baseUrl ?? ""}
      quickProviderIds={chatQuickIds}
      excludeProviderIds={["medousa-local"]}
      statusOk={visionStatusOk}
      statusLabel={visionStatusOk ? "Ready" : "Needs setup"}
      statusDetail={visionStatusOk
        ? "Image attachments will use this model."
        : "Pick a vision-capable model before sending images in chat."}
      showSuggestedModels
      disabled={readOnly || workshopDefaults.saving}
      onProviderChange={onVisionProviderChange}
      onModelChange={(value) =>
        (workshopDefaults.draft = {
          ...workshopDefaults.draft,
          inferenceProfiles: mergeProfiles({
            vision: {
              provider: visionProfile?.provider ?? "openai",
              model: value,
              baseUrl: visionProfile?.baseUrl ?? null,
              fallbacks: visionProfile?.fallbacks ?? [],
            },
          }),
        })}
      onBaseUrlChange={(value) =>
        (workshopDefaults.draft = {
          ...workshopDefaults.draft,
          inferenceProfiles: mergeProfiles({
            vision: {
              provider: visionProfile?.provider ?? "openai",
              model: visionProfile?.model ?? "gpt-4o-mini",
              baseUrl: value,
              fallbacks: visionProfile?.fallbacks ?? [],
            },
          }),
        })}
      onStatus={(message, ok) => {
        visionProviderStatus = message;
        if (ok === true) visionProviderStatus = message;
      }}
    />

    <SettingsInferenceProfile
      title="Dictation"
      subtitle="Transcribes the mic button — does not change who answers in chat."
      {catalog}
      providerId={workshopDefaults.draft.sttProvider ?? "openai"}
      model={workshopDefaults.draft.sttModel ?? defaultSttModel(workshopDefaults.draft.sttProvider ?? "openai")}
      baseUrl={workshopDefaults.draft.sttBaseUrl ?? ""}
      apiKey={workshopDefaults.sttApiKeyDraft}
      apiKeySet={workshopDefaults.sttApiKeySet}
      quickProviderIds={["openai", "groq"]}
      excludeProviderIds={["medousa-local", "ollama"]}
      statusOk={sttReady}
      statusLabel={sttStatusLabel}
      statusDetail={sttReady
        ? "Voice input in chat is ready."
        : (sttReason ?? "Pick a Whisper provider and add a key.")}
      disabled={readOnly || workshopDefaults.saving}
      onProviderChange={onSttProviderChange}
      onModelChange={(value) =>
        (workshopDefaults.draft = syncFlatFieldsFromProfiles({
          ...workshopDefaults.draft,
          sttModel: value,
          inferenceProfiles: mergeProfiles({
            stt: {
              provider: workshopDefaults.draft.sttProvider ?? "openai",
              model: value,
              baseUrl: workshopDefaults.draft.sttBaseUrl ?? null,
              fallbacks: workshopDefaults.draft.inferenceProfiles?.stt?.fallbacks ?? [],
            },
          }),
        }))}
      onApiKeyChange={(value) => (workshopDefaults.sttApiKeyDraft = value)}
      onBaseUrlChange={(value) =>
        (workshopDefaults.draft = syncFlatFieldsFromProfiles({
          ...workshopDefaults.draft,
          sttBaseUrl: value,
          inferenceProfiles: mergeProfiles({
            stt: {
              provider: workshopDefaults.draft.sttProvider ?? "openai",
              model: workshopDefaults.draft.sttModel ?? defaultSttModel(workshopDefaults.draft.sttProvider ?? "openai"),
              baseUrl: value,
              fallbacks: workshopDefaults.draft.inferenceProfiles?.stt?.fallbacks ?? [],
            },
          }),
        }))}
      onStatus={(message, ok) => {
        sttProviderStatus = message;
        if (ok === true) {
          sttProviderStatus = message;
          void refreshSttStatus();
        }
      }}
    />
  </div>

  {#if providerStatus}
    <p class="settings-inline-status">{providerStatus}</p>
  {/if}
  {#if visionProviderStatus}
    <p class="settings-inline-status">{visionProviderStatus}</p>
  {/if}
  {#if sttProviderStatus}
    <p class="settings-inline-status">{sttProviderStatus}</p>
  {/if}

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} onSaved={() => void refreshSttStatus()} />
  </div>
</section>
