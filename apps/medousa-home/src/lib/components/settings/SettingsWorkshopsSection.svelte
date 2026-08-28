<script lang="ts">
  import { onMount } from "svelte";
  import {
    Building2,
    Check,
    Ellipsis,
    FolderOpen,
    HardDrive,
    Home,
    Link2,
    Plus,
    Pencil,
  } from "@lucide/svelte";
  import WorkshopJoinSheet from "$lib/components/workshops/WorkshopJoinSheet.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    PERSONAL_WORKSHOP_ID,
    type WorkshopIcon,
    type WorkshopServer,
  } from "$lib/types/workshopRegistry";
  import { COLOR_THEME_OPTIONS, isColorThemeId } from "$lib/types/colorThemes";
  import { pickExternalFolder } from "$lib/utils/externalDeskApi";
  import {
    clearDelegationBinding,
    loadDelegationBinding,
    setDelegationBinding,
    type DelegationBinding,
  } from "$lib/utils/delegationApi";
  import { openGuide } from "$lib/guide/openGuide";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { isTauri } from "$lib/window";
  import { onThisHostPhrase } from "$lib/platformCopy";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  const ICON_OPTIONS: { id: WorkshopIcon; label: string }[] = [
    { id: "home", label: "Home" },
    { id: "building", label: "Team" },
    { id: "team", label: "Group" },
  ];

  interface Props {
    onDaemonHealth?: () => void | Promise<void>;
    /** When true, this is the lead story on Connection — no top rule. */
    lead?: boolean;
    mobile?: boolean;
  }

  let { onDaemonHealth, lead = false, mobile = false }: Props = $props();

  let renamingId = $state<string | null>(null);
  let renameDraft = $state("");
  let joinOpen = $state(false);
  let editingId = $state<string | null>(null);
  let brandingId = $state<string | null>(null);
  let brandColorDraft = $state("");
  let taglineDraft = $state("");
  let iconDraft = $state<WorkshopIcon>("home");
  let brandingBusy = $state(false);
  let brandingError = $state<string | null>(null);
  let addLocalOpen = $state(false);
  let localLabelDraft = $state("");
  let localDataDirDraft = $state("");
  let addLocalBusy = $state(false);
  let addLocalError = $state<string | null>(null);
  let delegationBinding = $state<DelegationBinding | null>(null);
  let delegationBusy = $state(false);
  let delegationError = $state<string | null>(null);
  let addMenuOpen = $state(false);
  let mobileManageId = $state<string | null>(null);
  let mobileManageMode = $state<"actions" | "rename" | "brand">("actions");
  let mobileSheetEl = $state<HTMLDivElement | null>(null);
  let mobileSheetHeaderEl = $state<HTMLElement | null>(null);

  const mobileManageWorkshop = $derived(
    workshops.workshops.find((workshop) => workshop.id === mobileManageId) ?? null,
  );
  const mobileSheetOpen = $derived(
    addMenuOpen || mobileManageId !== null || (mobile && addLocalOpen),
  );

  const isIosNative =
    typeof document !== "undefined" &&
    (document.documentElement.dataset.nativeShell === "ios" ||
      /iPhone|iPad|iPod/i.test(navigator.userAgent));

  onMount(() => {
    void (async () => {
      await workshops.load();
      if (isIosNative && workshops.activeWorkshopId === PERSONAL_WORKSHOP_ID) {
        try {
          delegationBinding = await loadDelegationBinding();
        } catch {
          delegationBinding = null;
        }
      }
    })();
  });

  function workshopIcon(icon: WorkshopIcon | undefined) {
    if (icon === "building" || icon === "team") return Building2;
    return Home;
  }

  function kindLabel(workshop: WorkshopServer): string {
    if (workshop.kind === "local" && workshop.id !== PERSONAL_WORKSHOP_ID) {
      return "Local engine";
    }
    return workshop.kind === "local" ? "This device" : "Paired portal";
  }

  function workshopMeta(workshop: WorkshopServer): string {
    const host = workshop.url.replace(/^https?:\/\//, "");
    const parts = [kindLabel(workshop), host];
    const theme = themeLabel(workshop.clientState?.colorThemeId);
    if (theme) parts.push(theme);
    return parts.join(" · ");
  }

  async function pickLocalDataDir() {
    const path = await pickExternalFolder();
    if (path) localDataDirDraft = path;
  }

  async function submitAddLocal() {
    const label = localLabelDraft.trim();
    const dataDir = localDataDirDraft.trim();
    if (!label || !dataDir) {
      addLocalError = "Name and engine folder are required.";
      return;
    }
    addLocalBusy = true;
    addLocalError = null;
    try {
      await workshops.addLocalEngine(label, dataDir);
      addLocalOpen = false;
      localLabelDraft = "";
      localDataDirDraft = "";
    } catch (err) {
      addLocalError = err instanceof Error ? err.message : String(err);
    } finally {
      addLocalBusy = false;
    }
  }

  function toggleEdit(workshopId: string) {
    if (editingId === workshopId) {
      editingId = null;
      brandingId = null;
      renamingId = null;
      return;
    }
    editingId = workshopId;
    brandingId = null;
    renamingId = null;
  }

  function startRename(workshop: WorkshopServer) {
    renamingId = workshop.id;
    brandingId = null;
    editingId = workshop.id;
    renameDraft = workshop.label;
  }

  async function commitRename(workshopId: string) {
    const label = renameDraft.trim();
    if (!label) {
      renamingId = null;
      return;
    }
    try {
      await workshops.renameWorkshop(workshopId, label);
    } catch {
      // Error surfaced on store.
    }
    renamingId = null;
  }

  async function switchTo(workshopId: string) {
    try {
      await workshops.selectWorkshop(workshopId, {
        onHealthChange: () => {
          void onDaemonHealth?.();
        },
      });
    } catch {
      // Error surfaced on store.
    }
  }

  function themeLabel(themeId: string | undefined): string | null {
    if (!themeId || !isColorThemeId(themeId)) return null;
    return COLOR_THEME_OPTIONS.find((option) => option.id === themeId)?.label ?? themeId;
  }

  function startBranding(workshop: WorkshopServer) {
    brandingId = workshop.id;
    renamingId = null;
    editingId = workshop.id;
    brandColorDraft = workshop.brandColor ?? "";
    taglineDraft = workshop.tagline ?? "";
    iconDraft = workshop.icon ?? (workshop.kind === "local" ? "home" : "building");
    brandingError = null;
  }

  async function saveBranding(workshopId: string) {
    brandingBusy = true;
    brandingError = null;
    try {
      await workshops.updateBranding(workshopId, {
        icon: iconDraft,
        brandColor: brandColorDraft.trim() || null,
        tagline: taglineDraft.trim() || null,
      });
      brandingId = null;
      return true;
    } catch (err) {
      brandingError = err instanceof Error ? err.message : String(err);
      return false;
    } finally {
      brandingBusy = false;
    }
  }

  async function removeWorkshop(workshopId: string) {
    const ok = window.confirm("Remove this workshop from the list? You can join again later.");
    if (!ok) return false;
    await workshops.removeWorkshop(workshopId, { onHealthChange: onDaemonHealth });
    editingId = null;
    return true;
  }

  async function useForDelegation(workshopId: string) {
    delegationBusy = true;
    delegationError = null;
    try {
      delegationBinding = await setDelegationBinding(workshopId);
    } catch (err) {
      delegationError = err instanceof Error ? err.message : String(err);
    } finally {
      delegationBusy = false;
    }
  }

  async function stopDelegation() {
    delegationBusy = true;
    delegationError = null;
    try {
      await clearDelegationBinding();
      delegationBinding = null;
    } catch (err) {
      delegationError = err instanceof Error ? err.message : String(err);
    } finally {
      delegationBusy = false;
    }
  }

  function openAddMenu() {
    haptic("light");
    mobileManageId = null;
    addMenuOpen = true;
  }

  function openMobileManage(workshop: WorkshopServer) {
    haptic("light");
    addMenuOpen = false;
    mobileManageId = workshop.id;
    mobileManageMode = "actions";
    editingId = workshop.id;
    brandingId = null;
    renamingId = null;
    brandingError = null;
  }

  function closeMobileSheet() {
    addMenuOpen = false;
    addLocalOpen = false;
    mobileManageId = null;
    mobileManageMode = "actions";
    editingId = null;
    brandingId = null;
    renamingId = null;
  }

  function chooseAddLocal() {
    addMenuOpen = false;
    addLocalError = null;
    addLocalOpen = true;
  }

  function chooseJoin() {
    addMenuOpen = false;
    joinOpen = true;
  }

  function startMobileRename(workshop: WorkshopServer) {
    renameDraft = workshop.label;
    mobileManageMode = "rename";
  }

  async function saveMobileRename(workshopId: string) {
    await commitRename(workshopId);
    closeMobileSheet();
  }

  function startMobileBranding(workshop: WorkshopServer) {
    startBranding(workshop);
    mobileManageMode = "brand";
  }

  async function saveMobileBranding(workshopId: string) {
    if (await saveBranding(workshopId)) closeMobileSheet();
  }

  async function removeMobileWorkshop(workshopId: string) {
    if (await removeWorkshop(workshopId)) closeMobileSheet();
  }

  $effect(() => {
    if (!mobileSheetOpen) return;
    return registerMobileBackHandler(() => {
      closeMobileSheet();
      return true;
    });
  });

  $effect(() => {
    if (!mobileSheetOpen || !mobileSheetEl) return;
    return attachMobileSheetGestures(mobileSheetEl, mobileSheetHeaderEl, {
      onDismiss: closeMobileSheet,
      swipeBack: false,
    });
  });
</script>

{#if isTauri()}
  <div class="ws-band" class:ws-band-lead={lead}>
    <div class="ws-band-head" class:ws-band-head-mobile={mobile}>
      <div class="ws-band-copy">
        <h3 class="settings-subsection-heading">{lead ? "Your workshops" : "Workshops"}</h3>
        <p class="settings-subsection-lead">
          One active connection — switch, or add another.
        </p>
        <button
          type="button"
          class="settings-learn-more"
          onclick={() => void openGuide("workshops-connections")}
        >
          Learn more
        </button>
      </div>
      {#if mobile}
        <button
          type="button"
          class="ws-add-btn"
          disabled={workshops.atWorkshopLimit}
          onclick={openAddMenu}
        >
          <Plus size={16} strokeWidth={1.9} aria-hidden="true" />
          Add workshop
        </button>
      {:else}
        <div class="ws-band-actions">
          <button
            type="button"
            class="ws-icon-btn"
            disabled={workshops.atWorkshopLimit}
            title="Add local engine"
            aria-label="Add local engine"
            onclick={() => {
              addLocalOpen = true;
              addLocalError = null;
            }}
          >
            <HardDrive size={15} strokeWidth={1.75} />
          </button>
          <button
            type="button"
            class="ws-icon-btn"
            disabled={workshops.atWorkshopLimit}
            title="Join paired workshop"
            aria-label="Join paired workshop"
            onclick={() => {
              joinOpen = true;
            }}
          >
            <Link2 size={15} strokeWidth={1.75} />
          </button>
        </div>
      {/if}
    </div>

    <div class="ws-stack" class:ws-stack-mobile={mobile}>
      {#each workshops.workshops as workshop (workshop.id)}
        {@const Icon = workshopIcon(workshop.icon)}
        {@const active = workshop.id === workshops.activeWorkshopId}
        {@const editing = editingId === workshop.id}
        <div class="ws-row" class:ws-row-mobile={mobile}>
          <div class="ws-tile" class:ws-tile-active={active} class:ws-tile-mobile={mobile}>
            <span class="ws-icon" aria-hidden="true">
              <Icon size={15} strokeWidth={1.75} />
            </span>
            <span class="ws-copy">
              {#if renamingId === workshop.id}
                <input
                  class="input w-full text-sm"
                  bind:value={renameDraft}
                  aria-label="Rename workshop"
                  onkeydown={(event) => {
                    if (event.key === "Enter") void commitRename(workshop.id);
                    if (event.key === "Escape") renamingId = null;
                  }}
                />
              {:else}
                <span class="ws-title">{workshop.label}</span>
                <span class="ws-meta">{workshopMeta(workshop)}</span>
              {/if}
            </span>
            <span class="ws-tile-actions">
              {#if renamingId === workshop.id}
                <button
                  type="button"
                  class="ws-cta"
                  onclick={() => void commitRename(workshop.id)}
                >
                  Save
                </button>
                <button type="button" class="ws-cta" onclick={() => (renamingId = null)}>
                  Cancel
                </button>
              {:else}
                {#if !active}
                  <button
                    type="button"
                    class="ws-cta {mobile ? 'ws-switch-btn' : ''}"
                    disabled={workshops.switching}
                    onclick={() => void switchTo(workshop.id)}
                  >
                    Switch
                  </button>
                {:else}
                  <span class="ws-pill">
                    {#if mobile}<Check size={13} strokeWidth={2.4} aria-hidden="true" />{/if}
                    Active
                  </span>
                {/if}
                <button
                  type="button"
                  class="ws-icon-btn"
                  class:ws-icon-btn-active={editing}
                  title={mobile ? "Manage workshop" : "Edit workshop"}
                  aria-label={mobile ? `Manage ${workshop.label}` : `Edit ${workshop.label}`}
                  aria-pressed={editing}
                  onclick={() => mobile ? openMobileManage(workshop) : toggleEdit(workshop.id)}
                >
                  {#if mobile}
                    <Ellipsis size={18} strokeWidth={1.9} />
                  {:else}
                    <Pencil size={14} strokeWidth={1.75} />
                  {/if}
                </button>
              {/if}
            </span>
          </div>

          {#if !mobile && editing && renamingId !== workshop.id}
            <div class="ws-edit">
              <div class="ws-actions">
                <button type="button" class="ws-cta" onclick={() => startRename(workshop)}>
                  Rename
                </button>
                <button type="button" class="ws-cta" onclick={() => startBranding(workshop)}>
                  Brand
                </button>
                {#if isIosNative && workshops.activeWorkshopId === PERSONAL_WORKSHOP_ID && workshop.pairing && (workshop.kind === "portal" || workshop.kind === "paired")}
                  {#if delegationBinding?.target.routeRef === workshop.id}
                    <button
                      type="button"
                      class="ws-cta"
                      disabled={delegationBusy}
                      onclick={() => void stopDelegation()}
                    >
                      Stop delegated work
                    </button>
                  {:else}
                    <button
                      type="button"
                      class="ws-cta"
                      disabled={delegationBusy}
                      onclick={() => void useForDelegation(workshop.id)}
                    >
                      Use for delegated work
                    </button>
                  {/if}
                {/if}
                {#if workshop.id !== PERSONAL_WORKSHOP_ID}
                  <button
                    type="button"
                    class="ws-cta ws-cta-danger"
                    disabled={workshops.switching}
                    onclick={() => void removeWorkshop(workshop.id)}
                  >
                    Remove
                  </button>
                {/if}
              </div>

              {#if delegationError}
                <p class="ws-footnote mt-2 text-content-warning">{delegationError}</p>
              {/if}

              {#if brandingId === workshop.id}
                <div class="ws-brand mt-3">
                  <p class="ws-footnote">Icon</p>
                  <div class="ws-actions">
                    {#each ICON_OPTIONS as option (option.id)}
                      <button
                        type="button"
                        class="btn btn-sm {iconDraft === option.id
                          ? 'variant-filled-primary'
                          : 'variant-ghost-surface'}"
                        onclick={() => {
                          iconDraft = option.id;
                        }}
                      >
                        {option.label}
                      </button>
                    {/each}
                  </div>
                  <label class="mt-3 block">
                    <span class="workshop-label">Accent color</span>
                    <input
                      class="input mt-1 w-full font-mono text-xs"
                      placeholder="#7C3AED"
                      bind:value={brandColorDraft}
                    />
                  </label>
                  <label class="mt-3 block">
                    <span class="workshop-label">Tagline</span>
                    <input
                      class="input mt-1 w-full text-sm"
                      maxlength={80}
                      placeholder="Acme engineering brain"
                      bind:value={taglineDraft}
                    />
                  </label>
                  <p class="ws-footnote mt-2">
                    Layout theme is set in Preferences while this workshop is active.
                  </p>
                  {#if brandingError}
                    <p class="mt-2 text-xs text-content-warning">{brandingError}</p>
                  {/if}
                  <div class="ws-actions mt-3">
                    <button
                      type="button"
                      class="btn btn-sm variant-soft"
                      disabled={brandingBusy}
                      onclick={() => void saveBranding(workshop.id)}
                    >
                      Save brand
                    </button>
                    <button
                      type="button"
                      class="btn btn-sm variant-ghost-surface"
                      onclick={() => {
                        brandingId = null;
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if workshops.error}
      <p class="mt-2 text-xs text-content-warning">{workshops.error}</p>
    {/if}
  </div>
{/if}

{#if mobile && addMenuOpen}
  <div
    class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeMobileSheet();
    }}
  >
    <div
      bind:this={mobileSheetEl}
      class="mobile-sheet mobile-turn-sheet mobile-sheet-medium settings-sheet"
      role="dialog"
      aria-label="Add workshop"
    >
      <header bind:this={mobileSheetHeaderEl} class="mobile-turn-sheet-header">
        <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
        <h2 class="mobile-turn-sheet-title">Add workshop</h2>
        <button type="button" class="ws-sheet-done" onclick={closeMobileSheet}>Done</button>
      </header>
      <div class="mobile-turn-sheet-body">
        <div class="mobile-turn-sheet-group">
          <button type="button" class="mobile-turn-sheet-row" onclick={chooseAddLocal}>
            <span class="ws-sheet-icon" aria-hidden="true">
              <HardDrive size={18} strokeWidth={1.8} />
            </span>
            <span class="mobile-turn-sheet-row-copy">
              <span class="mobile-turn-sheet-row-title">Create on this device</span>
              <span class="mobile-turn-sheet-row-subtitle">A separate local workshop and storage folder.</span>
            </span>
          </button>
          <button
            type="button"
            class="mobile-turn-sheet-row mobile-turn-sheet-row-divider"
            onclick={chooseJoin}
          >
            <span class="ws-sheet-icon" aria-hidden="true">
              <Link2 size={18} strokeWidth={1.8} />
            </span>
            <span class="mobile-turn-sheet-row-copy">
              <span class="mobile-turn-sheet-row-title">Join paired workshop</span>
              <span class="mobile-turn-sheet-row-subtitle">Connect using an invite or pairing link.</span>
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if mobile && mobileManageWorkshop}
  <div
    class="mobile-sheet-backdrop mobile-turn-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeMobileSheet();
    }}
  >
    <div
      bind:this={mobileSheetEl}
      class="mobile-sheet mobile-turn-sheet mobile-sheet-medium settings-sheet"
      role="dialog"
      aria-label="Manage {mobileManageWorkshop.label}"
    >
      <header bind:this={mobileSheetHeaderEl} class="mobile-turn-sheet-header">
        {#if mobileManageMode === "actions"}
          <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
        {:else}
          <button
            type="button"
            class="ws-sheet-back"
            onclick={() => (mobileManageMode = "actions")}
          >
            Back
          </button>
        {/if}
        <h2 class="mobile-turn-sheet-title">
          {mobileManageMode === "rename"
            ? "Rename"
            : mobileManageMode === "brand"
              ? "Appearance"
              : mobileManageWorkshop.label}
        </h2>
        <button type="button" class="ws-sheet-done" onclick={closeMobileSheet}>Done</button>
      </header>

      <div class="mobile-turn-sheet-body">
        {#if mobileManageMode === "actions"}
          <div class="mobile-turn-sheet-group">
            <button
              type="button"
              class="mobile-turn-sheet-row"
              onclick={() => startMobileRename(mobileManageWorkshop)}
            >
              <span class="mobile-turn-sheet-row-copy">
                <span class="mobile-turn-sheet-row-title">Rename</span>
                <span class="mobile-turn-sheet-row-subtitle">Change the name shown in Medousa.</span>
              </span>
            </button>
            <button
              type="button"
              class="mobile-turn-sheet-row mobile-turn-sheet-row-divider"
              onclick={() => startMobileBranding(mobileManageWorkshop)}
            >
              <span class="mobile-turn-sheet-row-copy">
                <span class="mobile-turn-sheet-row-title">Appearance</span>
                <span class="mobile-turn-sheet-row-subtitle">Choose its icon, accent, and tagline.</span>
              </span>
            </button>
            {#if isIosNative && workshops.activeWorkshopId === PERSONAL_WORKSHOP_ID && mobileManageWorkshop.pairing && (mobileManageWorkshop.kind === "portal" || mobileManageWorkshop.kind === "paired")}
              <button
                type="button"
                class="mobile-turn-sheet-row mobile-turn-sheet-row-divider"
                disabled={delegationBusy}
                onclick={() => delegationBinding?.target.routeRef === mobileManageWorkshop.id
                  ? void stopDelegation()
                  : void useForDelegation(mobileManageWorkshop.id)}
              >
                <span class="mobile-turn-sheet-row-copy">
                  <span class="mobile-turn-sheet-row-title">
                    {delegationBinding?.target.routeRef === mobileManageWorkshop.id
                      ? "Stop delegated work"
                      : "Use for delegated work"}
                  </span>
                  <span class="mobile-turn-sheet-row-subtitle">Choose where mobile coding work runs.</span>
                </span>
              </button>
            {/if}
          </div>

          {#if mobileManageWorkshop.id !== PERSONAL_WORKSHOP_ID}
            <div class="mobile-turn-sheet-group mobile-turn-sheet-group-secondary">
              <button
                type="button"
                class="mobile-turn-sheet-row ws-sheet-remove"
                disabled={workshops.switching}
                onclick={() => void removeMobileWorkshop(mobileManageWorkshop.id)}
              >
                <span class="mobile-turn-sheet-row-copy">
                  <span class="mobile-turn-sheet-row-title">Remove workshop</span>
                  <span class="mobile-turn-sheet-row-subtitle">You can pair it again later.</span>
                </span>
              </button>
            </div>
          {/if}

          {#if delegationError}
            <p class="mt-3 text-xs text-content-warning">{delegationError}</p>
          {/if}
        {:else if mobileManageMode === "rename"}
          <div class="mobile-turn-sheet-group ws-sheet-form">
            <label class="block">
              <span class="workshop-label">Workshop name</span>
              <input
                class="input mt-2 w-full text-sm"
                bind:value={renameDraft}
                aria-label="Workshop name"
                onkeydown={(event) => {
                  if (event.key === "Enter") void saveMobileRename(mobileManageWorkshop.id);
                }}
              />
            </label>
            <button
              type="button"
              class="btn variant-filled-primary mt-4 w-full"
              disabled={!renameDraft.trim()}
              onclick={() => void saveMobileRename(mobileManageWorkshop.id)}
            >
              Save name
            </button>
          </div>
        {:else}
          <div class="mobile-turn-sheet-group ws-sheet-form">
            <p class="workshop-label">Icon</p>
            <div class="ws-actions mt-2">
              {#each ICON_OPTIONS as option (option.id)}
                <button
                  type="button"
                  class="btn btn-sm {iconDraft === option.id
                    ? 'variant-filled-primary'
                    : 'variant-ghost-surface'}"
                  onclick={() => (iconDraft = option.id)}
                >
                  {option.label}
                </button>
              {/each}
            </div>
            <label class="mt-4 block">
              <span class="workshop-label">Accent color</span>
              <input
                class="input mt-1 w-full font-mono text-xs"
                placeholder="#7C3AED"
                bind:value={brandColorDraft}
              />
            </label>
            <label class="mt-4 block">
              <span class="workshop-label">Tagline</span>
              <input
                class="input mt-1 w-full text-sm"
                maxlength={80}
                placeholder="Acme engineering brain"
                bind:value={taglineDraft}
              />
            </label>
            {#if brandingError}
              <p class="mt-3 text-xs text-content-warning">{brandingError}</p>
            {/if}
            <button
              type="button"
              class="btn variant-filled-primary mt-4 w-full"
              disabled={brandingBusy}
              onclick={() => void saveMobileBranding(mobileManageWorkshop.id)}
            >
              {brandingBusy ? "Saving…" : "Save appearance"}
            </button>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if addLocalOpen}
  <div
    class={mobile
      ? "mobile-sheet-backdrop mobile-turn-sheet-backdrop"
      : "fixed inset-0 z-50 flex items-center justify-center bg-surface-950/80 p-4"}
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeMobileSheet();
    }}
  >
    <div
      bind:this={mobileSheetEl}
      class={mobile
        ? "mobile-sheet mobile-turn-sheet ws-add-local-sheet space-y-4 p-5"
        : "card w-full max-w-md space-y-4 p-5 shadow-xl"}
      role="dialog"
      aria-label="Add local engine"
    >
      <header bind:this={mobileSheetHeaderEl}>
        <h3 class="text-base font-semibold text-surface-50">Add local engine</h3>
        <p class="workshop-faint mt-1 text-sm">
          A second Medousa brain {onThisHostPhrase()} with its own storage folder and port.
        </p>
      </header>
      <label class="block space-y-1 text-sm">
        <span class="text-content-tertiary">Name</span>
        <input class="input w-full" placeholder="Work" bind:value={localLabelDraft} />
      </label>
      <div class="space-y-1">
        <span class="text-sm text-content-tertiary">Engine data folder</span>
        <div class="flex gap-2">
          <input
            class="input min-w-0 flex-1 font-mono text-xs"
            placeholder="/Users/you/MedousaWork"
            bind:value={localDataDirDraft}
          />
          <button
            type="button"
            class="btn btn-sm variant-soft-surface shrink-0"
            onclick={() => void pickLocalDataDir()}
          >
            <FolderOpen size={14} strokeWidth={2} />
            Choose
          </button>
        </div>
      </div>
      {#if addLocalError}
        <p class="text-sm text-content-warning">{addLocalError}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => {
            addLocalOpen = false;
          }}
        >
          Cancel
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={addLocalBusy}
          onclick={() => void submitAddLocal()}
        >
          {addLocalBusy ? "Creating…" : "Create engine"}
        </button>
      </div>
    </div>
  </div>
{/if}

<WorkshopJoinSheet
  open={joinOpen}
  variant={mobile ? "mobile" : "desktop"}
  onClose={() => {
    joinOpen = false;
  }}
  onHealthChange={onDaemonHealth}
/>

<style>
  .ws-band-lead,
  .ws-band:not(.ws-band-lead) {
    margin-top: 1.25rem;
  }

  .ws-band-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.6rem;
  }

  .ws-band-copy {
    min-width: 0;
    flex: 1 1 auto;
  }

  .ws-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .ws-band-head .settings-subsection-lead {
    margin-bottom: 0;
  }

  .ws-band-actions,
  .ws-tile-actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
  }

  .ws-add-btn {
    display: inline-flex;
    min-height: 2.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.38);
    border-radius: 0.7rem;
    background: rgb(var(--color-surface-800) / 0.62);
    padding: 0 0.85rem;
    color: rgb(var(--color-surface-100));
    font-size: 0.8rem;
    font-weight: 600;
  }

  .ws-add-btn:disabled {
    opacity: 0.4;
  }

  .ws-icon-btn {
    display: inline-flex;
    height: 1.85rem;
    width: 1.85rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.28);
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease,
      color 120ms ease;
  }

  .ws-icon-btn:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.5);
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .ws-icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .ws-icon-btn-active {
    border-color: rgb(var(--color-primary-500) / 0.45);
    color: rgb(var(--theme-link));
  }

  .ws-stack {
    display: grid;
    gap: 0.5rem;
  }

  .ws-row {
    display: grid;
    gap: 0.35rem;
  }

  .ws-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .ws-tile-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
  }

  .ws-icon {
    display: flex;
    height: 1.75rem;
    width: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--theme-text-secondary));
  }

  .ws-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .ws-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .ws-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .ws-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .ws-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .ws-cta-danger {
    color: rgb(var(--theme-error) / 0.9);
  }

  .ws-pill {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--theme-link));
  }

  .ws-edit {
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .ws-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .ws-footnote {
    margin: 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .ws-band-head-mobile {
    flex-direction: column;
    margin-bottom: 0.8rem;
  }

  .ws-band-head-mobile .ws-add-btn {
    width: 100%;
  }

  .ws-stack-mobile {
    gap: 0;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 1rem;
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .ws-row-mobile + .ws-row-mobile {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.26);
  }

  .ws-tile-mobile {
    min-height: 4.5rem;
    border: 0;
    border-radius: 0;
    background: transparent;
    padding: 0.75rem;
  }

  .ws-tile-mobile.ws-tile-active {
    background: rgb(var(--color-primary-500) / 0.055);
  }

  .ws-tile-mobile .ws-icon-btn {
    height: 2.75rem;
    width: 2.75rem;
    border-color: transparent;
    background: transparent;
  }

  .ws-switch-btn {
    min-height: 2.75rem;
    border-radius: 0.65rem;
    padding: 0 0.65rem;
  }

  .ws-switch-btn:active:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.45);
  }

  .ws-sheet-done,
  .ws-sheet-back {
    display: inline-flex;
    min-height: 2.75rem;
    min-width: 2.75rem;
    align-items: center;
    border: 0;
    background: transparent;
    color: rgb(var(--theme-link));
    font-size: 0.8rem;
    font-weight: 600;
  }

  .ws-sheet-done {
    justify-content: flex-end;
  }

  .ws-sheet-back {
    justify-content: flex-start;
  }

  .ws-sheet-icon {
    display: inline-flex;
    height: 2.5rem;
    width: 2.5rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.7rem;
    background: rgb(var(--color-surface-700) / 0.55);
    color: rgb(var(--theme-text-secondary));
  }

  .ws-sheet-remove :global(.mobile-turn-sheet-row-title) {
    color: rgb(var(--theme-error));
  }

  .ws-sheet-form {
    padding: 1rem;
  }

  .ws-add-local-sheet {
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
  }
</style>
