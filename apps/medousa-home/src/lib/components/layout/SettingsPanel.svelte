<script lang="ts">
  import SettingsNav from "$lib/components/settings/SettingsNav.svelte";
  import SettingsGeneralSection from "$lib/components/settings/SettingsGeneralSection.svelte";
  import SettingsAppearanceSection from "$lib/components/settings/SettingsAppearanceSection.svelte";
  import SettingsConnectionSection from "$lib/components/settings/SettingsConnectionSection.svelte";
  import SettingsIntegrationsSection from "$lib/components/settings/SettingsIntegrationsSection.svelte";
  import SettingsAdvancedSection from "$lib/components/settings/SettingsAdvancedSection.svelte";
  import type { DaemonHealth } from "$lib/daemon";
  import { settings, COLOR_THEME_OPTIONS } from "$lib/stores/settings.svelte";
  import type { SettingsSectionId } from "$lib/types/settings";

  interface Props {
    visible: boolean;
    revision: number;
    health: DaemonHealth | null;
    cronActiveCount: number;
    cronTotalCount: number;
    onOpenRuntime: () => void;
    onOpenMessaging?: () => void;
  onOpenCron?: () => void;
  onDaemonHealth: () => void | Promise<void>;
    mobile?: boolean;
    embedded?: boolean;
  }

  let {
    visible,
    revision,
    health,
    cronActiveCount,
    cronTotalCount,
    onOpenRuntime,
    onOpenMessaging,
    onOpenCron,
    onDaemonHealth,
    mobile = false,
    embedded = false,
  }: Props = $props();

  let activeSection = $state<SettingsSectionId>("general");

  $effect(() => {
    activeSection = mobile ? "connection" : "general";
  });

  const themeLabel = $derived(
    COLOR_THEME_OPTIONS.find((option) => option.id === settings.colorTheme)?.label ??
      "Theme",
  );

  const summaryParts = $derived(
    [
      health?.ok ? "Connected" : "Offline",
      `${themeLabel}${settings.darkMode ? " dark" : " light"}`,
      cronTotalCount > 0 ? `${cronActiveCount} cron active` : null,
    ].filter(Boolean),
  );
</script>

<section class="settings-panel flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if !embedded}
    <header class="workshop-header">
      <h1 class="text-base font-semibold text-surface-50">Settings</h1>
      <p class="settings-summary-strip mt-1 text-xs text-surface-300">
        {summaryParts.join(" · ")}
      </p>
    </header>
  {:else if mobile}
    <p class="settings-summary-strip border-b border-surface-500/35 px-4 py-2 text-xs text-surface-400">
      {summaryParts.join(" · ")}
    </p>
  {/if}

  <div class="settings-shell min-h-0 flex-1 {mobile ? 'flex flex-col' : 'flex'}">
    <aside class="settings-shell-nav shrink-0 {mobile ? 'px-4 pt-3' : 'border-r border-surface-500/35 p-3'}">
      <SettingsNav
        active={activeSection}
        {mobile}
        onSelect={(section) => {
          activeSection = section;
        }}
      />
    </aside>

    <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">
      {#if activeSection === "general"}
        <SettingsGeneralSection {mobile} />
      {:else if activeSection === "appearance"}
        <SettingsAppearanceSection />
      {:else if activeSection === "connection"}
        <SettingsConnectionSection {health} {onDaemonHealth} {mobile} />
      {:else if activeSection === "integrations"}
        <SettingsIntegrationsSection
          {health}
          {cronActiveCount}
          {cronTotalCount}
          {onOpenMessaging}
          {onOpenCron}
          {onOpenRuntime}
        />
      {:else}
        <SettingsAdvancedSection {revision} {health} {mobile} />
      {/if}
    </div>
  </div>
</section>
