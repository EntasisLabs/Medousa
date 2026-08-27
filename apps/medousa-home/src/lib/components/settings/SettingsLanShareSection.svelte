<script lang="ts">
  import { onMount } from "svelte";
  import PeerAvatar from "$lib/components/peers/PeerAvatar.svelte";
  import {
    loadConnectionPrefs,
    setPublicBind,
    type ConnectionPrefsSummary,
  } from "$lib/connection";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    downloadShareBundle,
    exportShareBundle,
    getLanPairingStatus,
    importShareBundle,
    listTrustedWorkshops,
    pushShareBundleToWorkshop,
    revokeTrustedWorkshop,
    setLanPairingEnabled,
    type LanPairingStatus,
    type ShareConflictStrategy,
    type ShareImportResult,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import {
    meshListLocalPeers,
    meshSetPeerRendezvous,
    meshSetPeerTaskRequest,
    type MeshPeerGrantRow,
  } from "$lib/utils/meshIntroApi";
  import { workshopBasementRestartHint } from "$lib/platformCopy";
  import {
    friendlySettingsError,
    isMissingCapabilityError,
  } from "$lib/utils/normieErrors";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { isTauri } from "$lib/window";
  import { ChevronDown, Share2, Upload } from "@lucide/svelte";

  interface Props {
    mobile?: boolean;
    /** Omit page chrome when nested under Sharing. */
    embedded?: boolean;
  }

  let { mobile = false, embedded = false }: Props = $props();

  let backupOpen = $state(false);

  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let meshPeers = $state<MeshPeerGrantRow[]>([]);
  let lanPairing = $state<LanPairingStatus | null>(null);
  let connectionPrefs = $state<ConnectionPrefsSummary | null>(null);
  let lanBusy = $state(false);
  let reachBusy = $state(false);
  let meshBusy = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let includeEnvironment = $state(true);
  let conflictStrategy = $state<ShareConflictStrategy>("rename");
  let lastBundle = $state<Record<string, unknown> | null>(null);
  let importInput: HTMLInputElement | undefined = $state();

  const CONFLICT_OPTIONS: {
    id: ShareConflictStrategy;
    label: string;
    hint: string;
  }[] = [
    { id: "rename", label: "Rename", hint: "Keep both — duplicates get a new name" },
    { id: "skip", label: "Skip", hint: "Leave existing views alone" },
    { id: "overwrite", label: "Overwrite", hint: "Replace matching views" },
  ];

  function openPeers() {
    if (mobile || layout.isMobile) {
      layout.openMore("peers");
      return;
    }
    layout.navigateDesktop("peers", { bump: true });
  }

  async function refreshTrusted() {
    if (!isTauri()) return;
    try {
      trusted = await listTrustedWorkshops();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Local registry probe — stay quiet when the host isn't ready yet.
      if (!isMissingCapabilityError(message)) {
        error = friendlySettingsError(message, "Peers");
      }
    }
  }

  async function refreshLanPairing() {
    if (!isTauri()) return;
    try {
      lanPairing = await getLanPairingStatus();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (!isMissingCapabilityError(message)) {
        error = friendlySettingsError(message, "Pairing window");
      }
    }
  }

  async function refreshConnectionPrefs() {
    if (!isTauri() || mobile) return;
    try {
      connectionPrefs = await loadConnectionPrefs();
    } catch {
      connectionPrefs = null;
    }
  }

  async function refreshMeshPeers() {
    if (!isTauri()) return;
    try {
      meshPeers = await meshListLocalPeers();
    } catch {
      // Host-only route — ignore when not on the workshop engine.
      meshPeers = [];
    }
  }

  async function toggleRendezvous(peer: MeshPeerGrantRow, enabled: boolean) {
    meshBusy = true;
    error = null;
    success = null;
    try {
      const updated = await meshSetPeerRendezvous(peer.deviceId, enabled);
      meshPeers = meshPeers.map((entry) =>
        entry.deviceId === updated.deviceId ? updated : entry,
      );
      success = enabled
        ? `Rendezvous on for ${updated.displayName}.`
        : `Rendezvous off for ${updated.displayName}.`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      meshBusy = false;
    }
  }

  async function toggleTaskRequest(peer: MeshPeerGrantRow, enabled: boolean) {
    meshBusy = true;
    error = null;
    success = null;
    try {
      const updated = await meshSetPeerTaskRequest(peer.deviceId, enabled);
      meshPeers = meshPeers.map((entry) =>
        entry.deviceId === updated.deviceId ? updated : entry,
      );
      success = enabled
        ? `Delegated work on for ${updated.displayName}.`
        : `Delegated work off for ${updated.displayName}.`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      meshBusy = false;
    }
  }

  async function togglePublicBind(enabled: boolean) {
    if (!isTauri() || mobile) return;
    reachBusy = true;
    error = null;
    success = null;
    try {
      const result = await setPublicBind(enabled);
      success = result.message;
      await refreshConnectionPrefs();
      await reconnectWorkshop(() => {});
    } catch (err) {
      error = friendlySettingsError(
        err instanceof Error ? err.message : String(err),
        "Wi‑Fi reachability",
      );
      await refreshConnectionPrefs();
    } finally {
      reachBusy = false;
    }
  }

  async function toggleLanPairing(enabled: boolean) {
    lanBusy = true;
    error = null;
    success = null;
    try {
      lanPairing = await setLanPairingEnabled(enabled);
      success = lanPairing.message;
    } catch (err) {
      error = friendlySettingsError(
        err instanceof Error ? err.message : String(err),
        "Pairing window",
      );
      await refreshLanPairing();
    } finally {
      lanBusy = false;
    }
  }

  onMount(() => {
    void refreshTrusted();
    void refreshLanPairing();
    void refreshConnectionPrefs();
    void refreshMeshPeers();
  });

  async function handleExport() {
    busy = true;
    error = null;
    success = null;
    try {
      const customSurfaces = (environment.spec?.surfaces ?? [])
        .filter((surface) => surface.kind === "custom")
        .map((surface) => surface.id);
      const bundle = await exportShareBundle({
        includeEnvironment,
        surfaceIds: includeEnvironment ? customSurfaces : [],
      });
      lastBundle = bundle;
      downloadShareBundle(bundle);
      success = "Bundle ready — downloaded, and ready to send to a peer.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handleImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    busy = true;
    error = null;
    success = null;
    try {
      const text = await file.text();
      const bundle = JSON.parse(text) as Record<string, unknown>;
      const result = await importShareBundle({ bundle, conflictStrategy });
      await environment.load();
      success = formatImportResult(result);
      lastBundle = bundle;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handlePush(workshopId: string) {
    if (!lastBundle) {
      error = "Export a bundle first, then send it.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = await pushShareBundleToWorkshop({
        workshopId,
        bundle: lastBundle,
        conflictStrategy,
      });
      success = `Sent to peer — ${formatImportResult(result)}`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function selectConflictStrategy(next: ShareConflictStrategy) {
    if (next === "overwrite" && conflictStrategy !== "overwrite") {
      const ok = window.confirm(
        "Overwrite replaces matching canvas views on import or send. Continue with Overwrite?",
      );
      if (!ok) return;
    }
    conflictStrategy = next;
  }

  async function revokeTrust(workshopId: string) {
    const ok = window.confirm(
      "Revoke trust for this peer? They will need to reconnect from Peers.",
    );
    if (!ok) return;
    busy = true;
    error = null;
    try {
      await revokeTrustedWorkshop(workshopId);
      await workshops.load();
      await refreshTrusted();
      success = "Trust revoked.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function formatImportResult(result: ShareImportResult): string {
    return `Imported ${result.surfacesImported} views, ${result.componentsImported} widgets, ${result.vaultNotesImported} notes, ${result.artifactsImported} artifacts.`;
  }

  function peerStatus(workshop: TrustedWorkshopSummary): "ready" | "reconnect" {
    return workshop.hasSessionToken ? "ready" : "reconnect";
  }

  function peerMeta(workshop: TrustedWorkshopSummary): string {
    if (!workshop.hasSessionToken) return "Needs reconnect";
    if (workshop.inbound) return "Connected to you";
    return "Trusted peer";
  }
</script>

<section class="nearby-calm" class:nearby-calm-spaced={!embedded}>
  {#if !embedded}
    <header class="settings-section-header mb-4">
      <h2 class="text-base font-semibold text-surface-50">Sharing</h2>
      <p class="workshop-faint mt-1 text-sm">
        Reachability, peers, and canvas backups.
      </p>
    </header>
  {/if}

  <div class="nearby-stack">
    {#if isTauri() && !mobile}
      <label class="nearby-tile">
        <span class="nearby-tile-copy">
          <span class="nearby-tile-title">Always reachable on Wi‑Fi</span>
          <span class="nearby-tile-meta">
            {#if connectionPrefs?.publicBind}
              Phones and peers can find this workshop without typing an IP
            {:else}
              {workshopBasementRestartHint()} without typing an IP
            {/if}
          </span>
        </span>
        <input
          type="checkbox"
          class="nearby-switch"
          checked={connectionPrefs?.publicBind ?? false}
          disabled={reachBusy || !connectionPrefs}
          onchange={(event) =>
            void togglePublicBind((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>
    {/if}

    <label class="nearby-tile">
      <span class="nearby-tile-copy">
        <span class="nearby-tile-title">Open pairing window</span>
        <span class="nearby-tile-meta">
          {#if !isTauri()}
            Managed on the workshop host
          {:else if lanPairing?.enabled}
            Open briefly — engine is listening on the network
          {:else}
            Temporary — private loopback when off
          {/if}
        </span>
      </span>
      <input
        type="checkbox"
        class="nearby-switch"
        checked={lanPairing?.enabled ?? false}
        disabled={lanBusy || !isTauri()}
        onchange={(event) =>
          void toggleLanPairing((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    <button type="button" class="nearby-tile nearby-tile-action" onclick={openPeers}>
      <span class="nearby-tile-copy">
        <span class="nearby-tile-title">Open Peers</span>
        <span class="nearby-tile-meta">Inbox and connect — never in the workshop switcher</span>
      </span>
      <span class="nearby-tile-cta">Open</span>
    </button>

    {#if trusted.length > 0}
      {#each trusted as workshop (workshop.workshopId)}
        <div class="nearby-tile">
          <PeerAvatar label={workshop.label} status={peerStatus(workshop)} />
          <span class="nearby-tile-copy">
            <span class="nearby-tile-title">{workshop.label}</span>
            <span class="nearby-tile-meta">{peerMeta(workshop)}</span>
          </span>
          <button
            type="button"
            class="nearby-tile-cta nearby-tile-cta-danger"
            disabled={busy}
            onclick={() => void revokeTrust(workshop.workshopId)}
          >
            Revoke
          </button>
        </div>
      {/each}
    {:else}
      <p class="nearby-empty">No trusted peers yet — open Peers to connect.</p>
    {/if}

    {#if meshPeers.length > 0}
      <details class="nearby-more">
        <summary class="nearby-more-summary">
          <span class="nearby-more-summary-copy">
            <span>Meet via this workshop</span>
            <span class="nearby-more-summary-meta">{meshPeers.length} clients</span>
          </span>
          <ChevronDown size={14} strokeWidth={2} class="nearby-more-chevron" aria-hidden="true" />
        </summary>
        <div class="nearby-more-body nearby-stack">
          <p class="nearby-footnote">
            Let paired clients introduce each other. Addresses stay private until both consent.
          </p>
          {#each meshPeers as peer (peer.deviceId)}
            <div class="nearby-stack">
              <label class="nearby-tile">
                <span class="nearby-tile-copy">
                  <span class="nearby-tile-title">{peer.displayName}</span>
                  <span class="nearby-tile-meta">Let this client introduce peers</span>
                </span>
                <input
                  type="checkbox"
                  class="nearby-switch"
                  checked={peer.rendezvous}
                  disabled={meshBusy}
                  aria-label="Rendezvous for {peer.displayName}"
                  onchange={(event) =>
                    void toggleRendezvous(
                      peer,
                      (event.currentTarget as HTMLInputElement).checked,
                    )}
                />
              </label>
              <label class="nearby-tile">
                <span class="nearby-tile-copy">
                  <span class="nearby-tile-title">Run delegated work</span>
                  <span class="nearby-tile-meta">Allow {peer.displayName} to send bounded tasks</span>
                </span>
                <input
                  type="checkbox"
                  class="nearby-switch"
                  checked={peer.taskRequest}
                  disabled={meshBusy}
                  aria-label="Delegated work for {peer.displayName}"
                  onchange={(event) =>
                    void toggleTaskRequest(
                      peer,
                      (event.currentTarget as HTMLInputElement).checked,
                    )}
                />
              </label>
            </div>
          {/each}
        </div>
      </details>
    {/if}

    <details class="nearby-more" bind:open={backupOpen}>
      <summary class="nearby-more-summary">
        <span class="nearby-more-summary-copy">
          <span>Canvas backup & send</span>
          <span class="nearby-more-summary-meta">
            {lastBundle ? "Bundle ready" : "Export, import, or send views"}
          </span>
        </span>
        <ChevronDown size={14} strokeWidth={2} class="nearby-more-chevron" aria-hidden="true" />
      </summary>
      <div class="nearby-more-body">
        <div class="nearby-stack">
          <label class="nearby-tile">
            <span class="nearby-tile-copy">
              <span class="nearby-tile-title">Include views</span>
              <span class="nearby-tile-meta">Custom canvas rooms and widgets</span>
            </span>
            <input
              type="checkbox"
              class="nearby-switch"
              bind:checked={includeEnvironment}
              disabled={busy}
            />
          </label>
        </div>

        <p class="nearby-footnote mt-3 mb-2">If names collide</p>
        <div class="nearby-choice-grid">
          {#each CONFLICT_OPTIONS as option (option.id)}
            <button
              type="button"
              class="nearby-choice"
              class:nearby-choice-active={conflictStrategy === option.id}
              disabled={busy}
              aria-pressed={conflictStrategy === option.id}
              onclick={() => selectConflictStrategy(option.id)}
            >
              <span class="nearby-choice-label">{option.label}</span>
              <span class="nearby-choice-hint">{option.hint}</span>
            </button>
          {/each}
        </div>

        <div class="nearby-actions mt-3">
          <button
            type="button"
            class="btn btn-sm variant-soft"
            disabled={busy}
            onclick={() => void handleExport()}
          >
            <Share2 size={14} />
            Export
          </button>
          <button
            type="button"
            class="btn btn-sm variant-soft"
            disabled={busy}
            onclick={() => importInput?.click()}
          >
            <Upload size={14} />
            Import
          </button>
          <input
            bind:this={importInput}
            type="file"
            accept="application/json,.json"
            class="hidden"
            onchange={handleImportFile}
          />
          {#if lastBundle}
            <span class="nearby-ready-pill">Ready to send</span>
          {/if}
        </div>

        {#if trusted.length > 0}
          <div class="nearby-stack mt-4">
            {#each trusted as workshop (workshop.workshopId)}
              <div class="nearby-tile">
                <PeerAvatar label={workshop.label} status={peerStatus(workshop)} />
                <span class="nearby-tile-copy">
                  <span class="nearby-tile-title">{workshop.label}</span>
                  <span class="nearby-tile-meta">
                    {#if !workshop.hasSessionToken}
                      Needs reconnect before send
                    {:else if lastBundle}
                      Send last bundle
                    {:else}
                      Export a bundle first
                    {/if}
                  </span>
                </span>
                <button
                  type="button"
                  class="nearby-tile-cta"
                  disabled={busy || !lastBundle || !workshop.hasSessionToken}
                  onclick={() => void handlePush(workshop.workshopId)}
                >
                  Send
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </details>

    {#if error}
      <p class="nearby-feedback nearby-feedback-error">{error}</p>
    {/if}
    {#if success}
      <p class="nearby-feedback nearby-feedback-ok">{success}</p>
    {/if}
  </div>
</section>

<style>
  .nearby-calm-spaced {
    margin-top: 0.25rem;
  }

  .nearby-stack {
    display: grid;
    gap: 0.5rem;
  }

  .nearby-tile {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .nearby-tile-action {
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .nearby-tile-action:hover {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .nearby-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .nearby-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .nearby-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .nearby-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .nearby-tile-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .nearby-tile-cta-danger {
    color: rgb(var(--theme-error) / 0.9);
  }

  .nearby-empty {
    margin: 0;
    padding: 0.1rem 0.15rem 0;
    font-size: 0.72rem;
    color: rgb(var(--theme-text-quiet));
  }

  .nearby-footnote {
    margin: 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .nearby-more {
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .nearby-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
    list-style: none;
  }

  .nearby-more-summary::-webkit-details-marker {
    display: none;
  }

  .nearby-more-summary-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .nearby-more-summary-meta {
    font-size: 0.68rem;
    font-weight: 500;
    color: rgb(var(--theme-text-quiet));
  }

  :global(.nearby-more-chevron) {
    transition: transform 160ms ease;
  }

  .nearby-more[open] :global(.nearby-more-chevron) {
    transform: rotate(180deg);
  }

  .nearby-more-body {
    padding: 0.65rem 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .nearby-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .nearby-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .nearby-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .nearby-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .nearby-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .nearby-choice-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.5rem;
  }

  @media (min-width: 720px) {
    .nearby-choice-grid {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  .nearby-choice {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-950) / 0.35);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .nearby-choice-active {
    border-color: rgb(var(--color-primary-500) / 0.4);
    background: rgb(var(--color-primary-500) / 0.1);
  }

  .nearby-choice-label {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .nearby-choice-hint {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .nearby-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .nearby-ready-pill {
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--theme-success));
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
    background: rgb(var(--color-success-500) / 0.12);
  }

  .nearby-feedback {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
  }

  .nearby-feedback-error {
    color: rgb(var(--theme-error));
  }

  .nearby-feedback-ok {
    color: rgb(var(--theme-success));
  }
</style>
