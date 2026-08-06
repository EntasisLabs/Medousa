<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    Brain,
    CalendarCheck,
    Check,
    Code2,
    LayoutDashboard,
    LoaderCircle,
    MessagesSquare,
    NotebookPen,
    PanelsTopLeft,
  } from "@lucide/svelte";
  import MedousaMark from "$lib/components/brand/MedousaMark.svelte";
  import WizardMigrationScreen from "$lib/components/wizard/WizardMigrationScreen.svelte";
  import WizardWelcomeScreen from "$lib/components/wizard/WizardWelcomeScreen.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { MEDOUSA_MARK_OPTIONS, medousaMarkOption } from "$lib/theme/medousaMarks";
  import {
    applyHomeOnboardingEnvironment,
    loadHomeOnboardingDraft,
    onboardingPackageIds,
    onboardingWorkspaceSurfaces,
    resetHomeOnboardingDraft,
    runHomeOnboardingTasks,
    saveHomeOnboardingDraft,
    type HomeOnboardingChannel,
    type HomeOnboardingDraft,
    type HomeOnboardingFocus,
    type HomeOnboardingLayout,
    type HomeOnboardingStage,
  } from "$lib/utils/homeOnboarding";
  import { setActiveLayoutTheme } from "$lib/utils/environmentLayout";
  import {
    fetchPackagesCatalog,
    installPackage,
    listenPackageProgress,
    type HomePackageRow,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import { isTauri } from "$lib/window";
  import { saveAssistantName, savePrincipalName } from "$lib/utils/onboardingIdentity";
  import "../wizard/wizardExperience.css";

  const FOCUS_OPTIONS: Array<{
    id: HomeOnboardingFocus;
    label: string;
    hint: string;
    icon: typeof Code2;
  }> = [
    { id: "code", label: "Build & code", hint: "Editor, terminal, and language tools", icon: Code2 },
    { id: "messaging", label: "Messages", hint: "Bring conversations into one place", icon: MessagesSquare },
    { id: "notes", label: "Notes & research", hint: "Library, browser, and daily notes", icon: NotebookPen },
    { id: "plan", label: "Plan & organize", hint: "Calendar, work board, and automations", icon: CalendarCheck },
    { id: "ai", label: "Work with Medousa", hint: "Private local brain or your model provider", icon: Brain },
  ];

  const CHANNEL_OPTIONS: Array<{ id: HomeOnboardingChannel; label: string }> = [
    { id: "discord", label: "Discord" },
    { id: "slack", label: "Slack" },
    { id: "telegram", label: "Telegram" },
    { id: "whatsapp", label: "WhatsApp" },
  ];

  const STAGES: HomeOnboardingStage[] = ["focus", "layout", "style", "brain", "ready"];
  const PACKAGE_LABELS: Record<string, string> = {
    "coding-engine": "Coding engine",
    langservers: "Language servers",
    "shell-session": "Shell session",
    "adapter-discord": "Discord adapter",
    "adapter-slack": "Slack adapter",
    "adapter-telegram": "Telegram adapter",
    "adapter-whatsapp": "WhatsApp adapter",
  };

  let draft = $state<HomeOnboardingDraft>(loadHomeOnboardingDraft());
  let homeName = $state("Home");
  let principalName = $state("");
  let saving = $state(false);
  let setupNotice = $state<string | null>(null);
  let packageRows = $state<HomePackageRow[]>([]);
  let packageProgress = $state<Record<string, PackageProgressEvent>>({});
  let packageFailures = $state<Record<string, string>>({});
  let packageInstalling = $state(false);
  let unlistenPackages: (() => void) | null = null;
  let previousRailExpanded = false;
  let homeLayoutApplied = false;

  const focusSet = $derived(new Set(draft.focus));
  const channelSet = $derived(new Set(draft.channels));
  const packageIds = $derived(onboardingPackageIds(draft.focus, draft.channels));
  const workspaceSurfaces = $derived(onboardingWorkspaceSurfaces(draft.focus));
  const selectedMark = $derived(medousaMarkOption(settings.medousaMark));
  const stageIndex = $derived(Math.max(0, STAGES.indexOf(draft.stage)));
  const visibleSteps = $derived(draft.focus.includes("ai") ? 5 : 4);
  const visibleStepIndex = $derived(
    draft.stage === "brain" ? 3 : draft.stage === "ready" ? visibleSteps - 1 : stageIndex,
  );

  $effect(() => {
    if (draft.stage === "brain" && wizard.uiPhase === "ready") {
      setStage("ready");
    } else if (draft.stage === "brain" && wizard.uiPhase === "mode") {
      setStage("style");
    }
  });

  onMount(() => {
    previousRailExpanded = layout.shellSidebarExpanded;
    if (wizard.screen !== "migration") {
      layout.setShellSidebarExpanded(false);
    }
    void workshops.load().then(() => {
      homeName = workshops.activeLabel?.trim() || "Home";
    });
    void fetchPackagesCatalog().then((catalog) => {
      packageRows = catalog?.packages ?? [];
    });
    if (isTauri()) {
      void listenPackageProgress((event) => {
        packageProgress = { ...packageProgress, [event.packageId]: event };
      }).then((unlisten) => {
        unlistenPackages = unlisten;
      });
    }
  });

  onDestroy(() => {
    unlistenPackages?.();
    if (!homeLayoutApplied) {
      layout.setShellSidebarExpanded(previousRailExpanded);
    }
  });

  function commitDraft(next: HomeOnboardingDraft) {
    draft = next;
    saveHomeOnboardingDraft(next);
  }

  function setStage(stage: HomeOnboardingStage) {
    commitDraft({ ...draft, stage });
  }

  function toggleFocus(id: HomeOnboardingFocus) {
    const next = new Set(draft.focus);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    const focus = FOCUS_OPTIONS.map((option) => option.id).filter((item) => next.has(item));
    const channels = focus.includes("messaging") ? draft.channels : [];
    const layout = focus.length <= 1 ? "focused" : "split";
    commitDraft({ ...draft, focus, channels, layout });
  }

  function toggleChannel(id: HomeOnboardingChannel) {
    const next = new Set(draft.channels);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    const channels = CHANNEL_OPTIONS.map((option) => option.id).filter((item) => next.has(item));
    commitDraft({ ...draft, channels });
  }

  function selectLayout(layout: HomeOnboardingLayout) {
    commitDraft({ ...draft, layout });
  }

  async function selectMark(markId: (typeof MEDOUSA_MARK_OPTIONS)[number]["id"]) {
    const option = medousaMarkOption(markId);
    settings.setMedousaMark(markId);
    settings.setColorTheme(option.pairedThemeId, { persistWorkshop: false });
  }

  async function saveHomeSetup() {
    saving = true;
    wizard.error = null;
    setupNotice = null;
    try {
      const label = homeName.trim() || "Home";
      savePrincipalName(principalName);
      saveAssistantName("Medousa");

      // Apply the visible result first. The daemon can still be warming during
      // first run, so persistence must never trap the operator on this step.
      shellTabs.applyHomeOnboardingLayout(draft.layout);
      layout.setShellSidebarExpanded(draft.layout !== "focused");
      layout.setActivityCollapsed(draft.layout !== "dashboard");
      layout.setVaultSidebarCollapsed(draft.layout === "focused");
      homeLayoutApplied = true;

      void runHomeOnboardingTasks([
        ...(isTauri() && workshops.activeWorkshopId && label !== workshops.activeLabel
          ? [
              {
                label: "Home name",
                run: () => workshops.renameWorkshop(workshops.activeWorkshopId!, label),
              },
            ]
          : []),
        {
          label: "Home layout",
          run: async () => {
            const spec = await environment.cloneCurrentSpec();
            applyHomeOnboardingEnvironment(spec, draft.focus, draft.layout);
            setActiveLayoutTheme(spec, { colorThemeId: selectedMark.pairedThemeId });
            await environment.saveSpec(spec);
          },
        },
      ]).then((deferred) => {
        if (deferred.length > 0) {
          setupNotice = "Your Home is ready. A few preferences will sync when Medousa reconnects.";
        }
      });

      void installSelectedPackages();

      await wizard.choosePreferredMode(draft.focus.includes("ai") ? "workspace-ai" : "workspace");
      setStage(draft.focus.includes("ai") ? "brain" : "ready");
    } catch (err) {
      wizard.error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function installSelectedPackages() {
    if (!isTauri() || packageInstalling) return;
    packageInstalling = true;
    for (const packageId of packageIds) {
      const row = packageRows.find((entry) => entry.id === packageId);
      if (row?.installed && !row.updateAvailable) continue;
      packageProgress = {
        ...packageProgress,
        [packageId]: {
          packageId,
          displayName: row?.displayName ?? PACKAGE_LABELS[packageId] ?? packageId,
          phase: "downloading",
          phaseLabel: "Downloading",
          percent: 0,
          message: "Starting…",
        },
      };
      try {
        await installPackage(packageId);
        packageProgress = {
          ...packageProgress,
          [packageId]: {
            ...packageProgress[packageId]!,
            phase: "ready",
            phaseLabel: "Ready",
            percent: 100,
            message: "Installed",
          },
        };
      } catch (err) {
        packageFailures = {
          ...packageFailures,
          [packageId]: err instanceof Error ? err.message : String(err),
        };
      }
    }
    packageInstalling = false;
  }

  async function finish() {
    // Background stores may hydrate while onboarding is open. Reassert the
    // chosen empty pane tree at the handoff so Home begins as a clean shell.
    shellTabs.applyHomeOnboardingLayout(draft.layout);
    await wizard.finish();
    resetHomeOnboardingDraft();
  }

  function back() {
    if (draft.stage === "layout") setStage("focus");
    else if (draft.stage === "style") setStage("layout");
    else if (draft.stage === "brain") setStage("style");
  }

  function packageLabel(packageId: string): string {
    return (
      packageRows.find((row) => row.id === packageId)?.displayName ??
      PACKAGE_LABELS[packageId] ??
      packageId
    );
  }
</script>

<section class="home-onboarding" aria-label="Set up Medousa Home">
  {#if wizard.screen === "migration"}
    <div class="home-onboarding-migration">
      <WizardMigrationScreen />
    </div>
  {:else}
    <header class="home-onboarding-header">
      <div class="home-onboarding-brand">
        <span class="home-onboarding-brand-mark" aria-hidden="true">
          <MedousaMark
            markId={settings.medousaMark}
            darkMode={settings.darkMode}
            simplified
            decorative
          />
        </span>
        <div>
          <p class="home-onboarding-kicker">Your Home</p>
          <p class="home-onboarding-title">{homeName || "Home"}</p>
        </div>
      </div>
      <div class="home-onboarding-progress" aria-label={`Step ${visibleStepIndex + 1} of ${visibleSteps}`}>
        {#each Array(visibleSteps) as _, index}
          <span class:home-onboarding-progress-on={index <= visibleStepIndex}></span>
        {/each}
      </div>
    </header>

    <div class="home-onboarding-body">
      {#if draft.stage === "focus"}
        <div class="home-onboarding-page wizard-stagger">
          <div class="wizard-beat home-onboarding-copy">
            <p class="home-onboarding-kicker">Start with what matters</p>
            <h1>Choose your initial workspace tools.</h1>
            <p>Start with the essentials or go all out—you can change everything later.</p>
          </div>
          <div class="wizard-beat home-onboarding-focus-grid">
            {#each FOCUS_OPTIONS as option (option.id)}
              {@const Icon = option.icon}
              <button
                type="button"
                class="home-onboarding-choice"
                class:home-onboarding-choice-active={focusSet.has(option.id)}
                aria-pressed={focusSet.has(option.id)}
                onclick={() => toggleFocus(option.id)}
              >
                <Icon size={20} strokeWidth={1.7} aria-hidden="true" />
                <span><strong>{option.label}</strong><small>{option.hint}</small></span>
                {#if focusSet.has(option.id)}<Check size={17} strokeWidth={2.4} aria-hidden="true" />{/if}
              </button>
            {/each}
          </div>
          {#if focusSet.has("messaging")}
            <div class="wizard-beat home-onboarding-channels">
              <p>Which channels should we prepare?</p>
              <div>
                {#each CHANNEL_OPTIONS as channel (channel.id)}
                  <button
                    type="button"
                    class:home-onboarding-chip-active={channelSet.has(channel.id)}
                    aria-pressed={channelSet.has(channel.id)}
                    onclick={() => toggleChannel(channel.id)}
                  >{channel.label}</button>
                {/each}
              </div>
            </div>
          {/if}
          <div class="home-onboarding-actions">
            <button
              type="button"
              class="btn variant-filled-primary wizard-cta min-h-11 px-7"
              disabled={draft.focus.length === 0}
              onclick={() => setStage("layout")}
            >Shape my Home</button>
          </div>
        </div>
      {:else if draft.stage === "layout"}
        <div class="home-onboarding-page wizard-stagger">
          <div class="wizard-beat home-onboarding-copy">
            <p class="home-onboarding-kicker">Layout</p>
            <h1>Start with a shape that fits</h1>
            <p class="home-layout-subheadline">This becomes your initial layout—you can split, merge, or rearrange anytime.</p>
          </div>
          <div class="wizard-beat home-layout-workbench">
            <div class="home-layout-preview home-layout-{draft.layout}" aria-label={`${draft.layout} layout preview`}>
              {#each workspaceSurfaces.slice(0, 4) as surfaceId, index (surfaceId)}
                <div class="home-layout-pane">
                  <span>{surfaceId === "messaging" ? "Messages" : surfaceId === "artifacts" ? "Artifacts" : surfaceId === "files" ? "Files" : surfaceId === "notes" || surfaceId === "library" ? "Notes" : surfaceId}</span>
                  <small>{index === 0 ? "Primary" : "Nearby"}</small>
                </div>
              {/each}
            </div>
            <div class="home-layout-options">
              <button type="button" aria-pressed={draft.layout === "focused"} onclick={() => selectLayout("focused")}>
                <PanelsTopLeft size={18} /><span><strong>Focused</strong><small>One generous pane</small></span>
              </button>
              <button type="button" aria-pressed={draft.layout === "split"} onclick={() => selectLayout("split")}>
                <PanelsTopLeft size={18} /><span><strong>Side by side</strong><small>Two equal working panes</small></span>
              </button>
              <button type="button" aria-pressed={draft.layout === "dashboard"} onclick={() => selectLayout("dashboard")}>
                <LayoutDashboard size={18} /><span><strong>Dashboard</strong><small>Primary pane plus two companions</small></span>
              </button>
            </div>
          </div>
          <div class="home-onboarding-actions home-onboarding-actions-between">
            <button type="button" class="btn variant-ghost min-h-11" onclick={back}>Back</button>
            <button type="button" class="btn variant-filled-primary wizard-cta min-h-11 px-7" onclick={() => setStage("style")}>Choose the look</button>
          </div>
        </div>
      {:else if draft.stage === "style"}
        <div class="home-onboarding-page home-onboarding-style-page wizard-stagger">
          <div
            class="wizard-beat home-style-hero"
            style:--mark-stage-bg={settings.darkMode ? selectedMark.darkPreviewBackground : selectedMark.lightPreviewBackground}
            style:--mark-stage-fg={settings.darkMode ? selectedMark.darkPreviewForeground : selectedMark.lightPreviewForeground}
          >
            <div class="home-style-mark">
              <MedousaMark markId={settings.medousaMark} darkMode={settings.darkMode} />
            </div>
            <p>{selectedMark.label}</p>
            <small>{selectedMark.tagline}</small>
          </div>
          <div class="wizard-beat home-style-controls">
            <div class="home-onboarding-copy">
              <p class="home-onboarding-kicker">Make it yours</p>
              <h1>Choose your Medousa</h1>
              <p>Each mark previews with its matching color theme. They stay independently editable in Settings.</p>
            </div>
            <div class="home-style-fields">
              <label><span>Home name</span><input class="input" bind:value={homeName} maxlength={48} /></label>
              <label><span>Your name <small>optional</small></span><input class="input" bind:value={principalName} maxlength={40} placeholder="What should Medousa call you?" /></label>
            </div>
            <div class="home-style-grid" role="listbox" aria-label="Choose Medousa">
              {#each MEDOUSA_MARK_OPTIONS as option (option.id)}
                <button
                  type="button"
                  role="option"
                  aria-selected={settings.medousaMark === option.id}
                  class:home-style-option-active={settings.medousaMark === option.id}
                  style:--mark-option-bg={settings.darkMode ? option.darkPreviewBackground : option.lightPreviewBackground}
                  style:--mark-option-accent={settings.darkMode ? option.darkColor : option.lightColor}
                  onclick={() => void selectMark(option.id)}
                >
                  <span><MedousaMark markId={option.id} darkMode={settings.darkMode} decorative /></span>
                  <small>{option.label}</small>
                </button>
              {/each}
            </div>
            <div class="home-onboarding-actions home-onboarding-actions-between">
              <button type="button" class="btn variant-ghost min-h-11" disabled={saving} onclick={back}>Back</button>
              <button
                type="button"
                class="btn variant-filled-primary wizard-cta min-h-11 px-7"
                disabled={saving || !homeName.trim()}
                onclick={() => void saveHomeSetup()}
              >{saving ? "Building Home…" : "Make it mine"}</button>
            </div>
            {#if wizard.error}
              <p class="home-onboarding-error" role="alert">{wizard.error}</p>
            {/if}
          </div>
        </div>
      {:else if draft.stage === "brain"}
        <div class="home-onboarding-brain">
          <WizardWelcomeScreen />
        </div>
      {:else}
        <div class="home-onboarding-page home-ready-page wizard-stagger">
          <div
            class="wizard-beat home-ready-mark"
            style:--mark-stage-bg={settings.darkMode ? selectedMark.darkPreviewBackground : selectedMark.lightPreviewBackground}
          >
            <MedousaMark markId={settings.medousaMark} darkMode={settings.darkMode} />
          </div>
          <div class="wizard-beat home-onboarding-copy home-ready-copy">
            <p class="home-onboarding-kicker">Ready when you are</p>
            <h1>{homeName} is yours.</h1>
            {#if setupNotice}
              <p role="status">Jump in now. We’ll sync the finishing touches when Medousa reconnects.</p>
            {:else}
              <p>Jump in now. Anything still setting up will finish quietly in the background.</p>
            {/if}
          </div>
          {#if packageIds.length > 0}
            <div class="wizard-beat home-package-list" aria-live="polite">
              {#each packageIds as packageId (packageId)}
                {@const progress = packageProgress[packageId]}
                <div class="home-package-row">
                  <span>{packageLabel(packageId)}</span>
                  {#if packageFailures[packageId]}
                    <small class="text-content-warning">Try again in Settings → Packages</small>
                  {:else if progress?.percent === 100}
                    <small class="text-content-success">Ready</small>
                  {:else}
                    <span class="home-package-progress"><i style:width={`${Math.max(3, progress?.percent ?? 0)}%`}></i></span>
                    <small>{Math.round(progress?.percent ?? 0)}%</small>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
          <div class="wizard-beat home-onboarding-actions">
            <button type="button" class="btn variant-filled-primary wizard-cta min-h-12 px-10" disabled={wizard.busy} onclick={() => void finish()}>Enter Home →</button>
          </div>
        </div>
      {/if}
    </div>

    <footer class="home-onboarding-footer">
      <span>{draft.focus.length} focus area{draft.focus.length === 1 ? "" : "s"}</span>
      <span>Everything stays editable</span>
    </footer>
  {/if}
</section>

<style>
  .home-onboarding {
    position: absolute;
    inset: 0;
    z-index: 35;
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    background: rgb(var(--shell-canvas-bg, var(--color-surface-950)));
    color: rgb(var(--color-surface-50));
  }

  .home-onboarding-migration,
  .home-onboarding-brain {
    width: min(42rem, calc(100% - 2rem));
    height: min(42rem, calc(100% - 2rem));
    margin: auto;
    padding: 2rem;
    overflow-y: auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 1rem;
    background: rgb(var(--color-surface-900) / 0.9);
  }

  .home-onboarding-header,
  .home-onboarding-footer {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.75rem 1.25rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.25);
    background: rgb(var(--shell-header-bg, var(--color-surface-900)) / 0.68);
    backdrop-filter: blur(18px);
  }

  .home-onboarding-footer {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.25);
    border-bottom: 0;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-quiet));
  }

  .home-onboarding-brand { display: flex; align-items: center; gap: 0.65rem; }
  .home-onboarding-brand-mark { width: 1.6rem; height: 2rem; }
  .home-onboarding-kicker { margin: 0; font-size: 0.65rem; font-weight: 650; letter-spacing: 0.16em; text-transform: uppercase; color: rgb(var(--theme-link)); }
  .home-onboarding-title { margin: 0.08rem 0 0; font-size: 0.86rem; font-weight: 600; }
  .home-onboarding-progress { display: flex; width: min(14rem, 38vw); gap: 0.3rem; }
  .home-onboarding-progress span { height: 3px; flex: 1; border-radius: 999px; background: rgb(var(--color-surface-500) / 0.25); }
  .home-onboarding-progress .home-onboarding-progress-on { background: rgb(var(--color-primary-400) / 0.82); }

  .home-onboarding-body { min-height: 0; flex: 1; overflow-y: auto; padding: clamp(1.25rem, 3vw, 2.5rem); }
  .home-onboarding-page { display: flex; width: min(58rem, 100%); min-height: 100%; margin: 0 auto; flex-direction: column; justify-content: center; }
  .home-onboarding-copy { max-width: 39rem; }
  .home-onboarding-copy h1 { margin: 0.45rem 0 0; font-size: clamp(1.7rem, 3vw, 2.45rem); font-weight: 650; letter-spacing: -0.035em; }
  .home-onboarding-copy > p:last-child { margin: 0.65rem 0 0; max-width: 36rem; color: rgb(var(--theme-text-secondary)); line-height: 1.55; }
  .home-onboarding-copy > .home-layout-subheadline { font-size: 1rem; font-weight: 600; color: rgb(var(--color-surface-200)); }

  .home-onboarding-focus-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.75rem; margin-top: 1.6rem; }
  .home-onboarding-choice { display: flex; min-width: 0; align-items: flex-start; gap: 0.75rem; padding: 1rem; border: 1px solid rgb(var(--color-surface-500) / 0.3); border-radius: 0.8rem; background: rgb(var(--color-surface-900) / 0.48); text-align: left; color: rgb(var(--theme-text-secondary)); }
  .home-onboarding-choice:hover, .home-onboarding-choice-active { border-color: rgb(var(--color-primary-500) / 0.55); background: rgb(var(--color-primary-500) / 0.09); color: rgb(var(--color-primary-200)); }
  .home-onboarding-choice > span { min-width: 0; flex: 1; }
  .home-onboarding-choice strong, .home-layout-options strong { display: block; font-size: 0.86rem; font-weight: 600; color: rgb(var(--color-surface-50)); }
  .home-onboarding-choice small, .home-layout-options small { display: block; margin-top: 0.22rem; color: rgb(var(--theme-text-tertiary)); font-size: 0.72rem; line-height: 1.4; }
  .home-onboarding-channels { margin-top: 1rem; padding: 0.85rem 1rem; border-radius: 0.75rem; background: rgb(var(--color-surface-900) / 0.42); }
  .home-onboarding-channels p { margin: 0 0 0.55rem; font-size: 0.72rem; color: rgb(var(--theme-text-secondary)); }
  .home-onboarding-channels > div { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .home-onboarding-channels button { padding: 0.35rem 0.65rem; border: 1px solid rgb(var(--color-surface-500) / 0.35); border-radius: 999px; font-size: 0.72rem; color: rgb(var(--theme-text-secondary)); }
  .home-onboarding-channels .home-onboarding-chip-active { border-color: rgb(var(--color-primary-500) / 0.55); background: rgb(var(--color-primary-500) / 0.13); color: rgb(var(--color-primary-200)); }
  .home-onboarding-actions { display: flex; justify-content: flex-end; gap: 0.75rem; margin-top: 1.6rem; }
  .home-onboarding-actions-between { justify-content: space-between; }
  .home-onboarding-error { margin: 0.7rem 0 0; color: rgb(var(--theme-error)); font-size: 0.75rem; }

  .home-layout-workbench { display: grid; grid-template-columns: minmax(0, 1fr) 15rem; gap: 1rem; margin-top: 1.5rem; }
  .home-layout-preview { display: grid; min-height: 18rem; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; gap: 0.45rem; padding: 0.6rem; border: 1px solid rgb(var(--color-surface-500) / 0.25); border-radius: 0.85rem; background: rgb(var(--color-surface-900) / 0.42); }
  .home-layout-pane { display: flex; min-width: 0; flex-direction: column; justify-content: space-between; padding: 0.8rem; border: 1px solid rgb(var(--color-surface-500) / 0.3); border-radius: 0.65rem; background: rgb(var(--color-surface-900) / 0.8); text-transform: capitalize; }
  .home-layout-pane small { color: rgb(var(--theme-text-quiet)); font-size: 0.65rem; }
  .home-layout-focused .home-layout-pane:first-child { grid-area: 1 / 1 / 3 / 3; }
  .home-layout-focused .home-layout-pane:not(:first-child) { display: none; }
  .home-layout-split .home-layout-pane:first-child { grid-area: 1 / 1 / 3 / 2; }
  .home-layout-split .home-layout-pane:nth-child(2) { grid-area: 1 / 2 / 3 / 3; }
  .home-layout-split .home-layout-pane:nth-child(n+3) { display: none; }
  .home-layout-dashboard .home-layout-pane:first-child { grid-row: 1 / 3; }
  .home-layout-dashboard .home-layout-pane:nth-child(n+4) { display: none; }
  .home-layout-options { display: flex; flex-direction: column; gap: 0.55rem; }
  .home-layout-options button { display: flex; align-items: flex-start; gap: 0.65rem; padding: 0.8rem; border: 1px solid rgb(var(--color-surface-500) / 0.28); border-radius: 0.7rem; background: rgb(var(--color-surface-900) / 0.45); text-align: left; color: rgb(var(--theme-text-tertiary)); }
  .home-layout-options button[aria-pressed="true"] { border-color: rgb(var(--color-primary-500) / 0.58); background: rgb(var(--color-primary-500) / 0.1); color: rgb(var(--color-primary-200)); }

  .home-onboarding-style-page { display: grid; grid-template-columns: minmax(15rem, 0.7fr) minmax(0, 1.3fr); gap: clamp(1.5rem, 4vw, 3.5rem); align-items: center; }
  .home-style-hero { display: grid; min-height: 27rem; place-items: center; align-content: center; padding: 1.5rem; border-radius: 1rem; background: var(--mark-stage-bg); color: var(--mark-stage-fg); box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08), 0 10px 24px rgb(0 0 0 / 0.08); text-align: center; }
  .home-style-mark { width: 8.5rem; height: 16rem; }
  .home-style-hero p { margin: 1rem 0 0; color: var(--mark-stage-fg); font-size: 0.85rem; letter-spacing: 0.14em; text-transform: uppercase; }
  .home-style-hero small { margin-top: 0.3rem; color: color-mix(in srgb, var(--mark-stage-fg) 62%, transparent); }
  .home-style-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 0.65rem; margin-top: 1rem; }
  .home-style-fields label { display: grid; gap: 0.35rem; font-size: 0.68rem; color: rgb(var(--theme-text-tertiary)); }
  .home-style-grid { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 0.45rem; margin-top: 1rem; }
  .home-style-grid button { display: grid; min-width: 0; justify-items: center; gap: 0.3rem; padding: 0.38rem; border: 1px solid transparent; border-radius: 0.6rem; color: rgb(var(--theme-text-tertiary)); transition: transform 160ms ease, border-color 160ms ease, background-color 160ms ease; }
  .home-style-grid button:hover, .home-style-grid .home-style-option-active { transform: translateY(-1px); border-color: color-mix(in srgb, var(--mark-option-accent) 56%, transparent); background: color-mix(in srgb, var(--mark-option-accent) 8%, transparent); }
  .home-style-grid button > span { display: grid; width: 100%; height: 4.5rem; place-items: center; padding: 0.45rem; border: 1px solid color-mix(in srgb, var(--mark-option-accent) 22%, transparent); border-radius: 0.42rem; background: var(--mark-option-bg); box-shadow: 0 2px 6px rgb(0 0 0 / 0.06); transition: transform 160ms ease, box-shadow 160ms ease; }
  .home-style-grid button:hover > span, .home-style-grid .home-style-option-active > span { transform: translateY(-1px) scale(1.02); box-shadow: 0 8px 16px rgb(0 0 0 / 0.1), 0 0 0 1px color-mix(in srgb, var(--mark-option-accent) 18%, transparent); }
  .home-style-grid button small { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.58rem; }

  .home-ready-page { align-items: center; padding-block: 0.75rem; text-align: center; }
  .home-ready-mark { width: 5.25rem; height: 7.5rem; padding: 0.6rem; border-radius: 0.8rem; background: var(--mark-stage-bg); }
  .home-ready-copy { max-width: 35rem; margin-top: 0.8rem; }
  .home-ready-copy h1 { margin-top: 0.35rem; font-size: clamp(1.55rem, 2.6vw, 2rem); line-height: 1.12; text-wrap: balance; }
  .home-ready-copy > p:last-child { max-width: 32rem; margin: 0.5rem auto 0; font-size: 0.9rem; line-height: 1.45; text-wrap: balance; }
  .home-package-list { width: min(32rem, 100%); margin-top: 0.9rem; padding: 0.65rem 0.85rem; border: 1px solid rgb(var(--color-surface-500) / 0.25); border-radius: 0.75rem; background: rgb(var(--color-surface-900) / 0.42); text-align: left; }
  .home-ready-page .home-onboarding-actions { margin-top: 1rem; }
  .home-package-row { display: flex; align-items: center; gap: 0.75rem; min-height: 1.8rem; font-size: 0.75rem; }
  .home-package-row + .home-package-row { border-top: 1px solid rgb(var(--color-surface-500) / 0.18); }
  .home-package-row > span:first-child { min-width: 8rem; flex: 1; }
  .home-package-row small { color: rgb(var(--theme-text-tertiary)); }
  .home-package-progress { width: 7rem; height: 4px; overflow: hidden; border-radius: 999px; background: rgb(var(--color-surface-600) / 0.5); }
  .home-package-progress i { display: block; height: 100%; background: rgb(var(--color-primary-500)); transition: width 180ms ease; }

  @media (prefers-reduced-motion: reduce) {
    .home-style-grid button,
    .home-style-grid button > span {
      transition: none;
    }
  }

  @media (max-width: 780px) {
    .home-onboarding-focus-grid, .home-style-fields { grid-template-columns: 1fr; }
    .home-layout-workbench, .home-onboarding-style-page { grid-template-columns: 1fr; }
    .home-layout-options { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); }
    .home-style-hero { min-height: 16rem; }
    .home-style-mark { width: 6rem; height: 10rem; }
  }

  @media (prefers-reduced-motion: reduce) {
    .home-package-progress i { transition: none; }
  }
</style>
