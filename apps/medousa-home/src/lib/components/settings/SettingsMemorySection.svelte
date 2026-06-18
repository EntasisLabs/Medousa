<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import MemoryAndYouPanel from "$lib/components/settings/MemoryAndYouPanel.svelte";
  import UserProfilesPanel from "$lib/components/settings/UserProfilesPanel.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const fields = [
    {
      key: "sliceHotWindowTurns" as const,
      label: "Recent turns kept vivid",
      hint: "How many recent turns stay in hot memory — the conversation she holds closest.",
      min: 2,
      max: 32,
    },
    {
      key: "sliceColdWindowTurns" as const,
      label: "How far back she can recall",
      hint: "In a long thread, how many older turns can still surface from cold storage.",
      min: 4,
      max: 64,
    },
    {
      key: "activationLongSessionTurnThreshold" as const,
      label: "When a chat becomes long",
      hint: "After this many turns, Medousa treats the session differently — tighter context rules.",
      min: 8,
      max: 80,
    },
    {
      key: "activationLongSessionMaxPromptChars" as const,
      label: "Extra context for long chats",
      hint: "Character budget when a session crosses the long-chat threshold.",
      min: 200,
      max: 2000,
      step: 20,
    },
  ];

  function numField(key: (typeof fields)[number]["key"], event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      [key]: Number.isFinite(value) ? value : null,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Memory</h2>
    <p class="workshop-faint mt-1 text-sm">
      Who she knows you are — and how long she keeps the conversation close.
    </p>
  </header>

  <UserProfilesPanel {mobile} />

  <MemoryAndYouPanel {mobile} />

  <div class="mt-8 border-t border-surface-500/35 pt-8">
    <h3 class="text-sm font-semibold text-surface-100">Conversation memory</h3>
    <p class="workshop-faint mt-1 text-xs">
      How long she keeps recent chat turns vivid — separate from who you are.
    </p>
  </div>

  <div class="mt-5 space-y-5">
    {#each fields as field (field.key)}
      <label class="block">
        <span class="block text-sm font-medium text-surface-100">{field.label}</span>
        <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">{field.hint}</span>
        <input
          type="number"
          class="input mt-2 w-full max-w-xs"
          min={field.min}
          max={field.max}
          step={field.step ?? 1}
          value={workshopDefaults.draft[field.key] ?? ""}
          readonly={readOnly}
          disabled={readOnly}
          oninput={(event) => numField(field.key, event)}
        />
      </label>
    {/each}
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <SettingsCharterSaveBar {mobile} />
  </div>
</section>
