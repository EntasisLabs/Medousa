<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import ShareToPeerSheet from "$lib/components/settings/ShareToPeerSheet.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    capabilityBadges,
    discoverLanWorkshops,
    downloadShareBundle,
    exportShareBundle,
    importShareBundle,
    listTrustedWorkshops,
    peerListMessages,
    peerMarkRead,
    peerSendMessage,
    peerUnreadCount,
    pushShareBundleToWorkshop,
    revokeTrustedWorkshop,
    trustWorkshopFromQr,
    type DiscoveredWorkshop,
    type PeerMessage,
    type ShareConflictStrategy,
    type ShareImportResult,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import { isTauri } from "$lib/window";
  import { RefreshCw, Share2, ShieldCheck, Upload } from "@lucide/svelte";

  let nearby = $state<DiscoveredWorkshop[]>([]);
  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let inbox = $state<PeerMessage[]>([]);
  let unread = $state(0);
  let loadingNearby = $state(false);
  let loadingTrusted = $state(false);
  let loadingInbox = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let includeEnvironment = $state(true);
  let conflictStrategy = $state<ShareConflictStrategy>("rename");
  let lastBundle = $state<Record<string, unknown> | null>(null);
  let trustQrUrl = $state("");
  let trustDaemonUrl = $state("");
  let trustName = $state("");
  let trustTarget = $state<DiscoveredWorkshop | null>(null);
  let pushWorkshopId = $state("");
  let composeWorkshopId = $state("");
  let composeBody = $state("");
  let composeAttachKind = $state<"none" | "note" | "artifact">("none");
  let composeNotePath = $state("");
  let composeArtifactId = $state("");
  let selectedMessageId = $state<string | null>(null);
  let shareSheetOpen = $state(false);
  let shareArtifactId = $state<string | null>(null);
  let shareVaultPath = $state<string | null>(null);
  let shareLabel = $state<string | null>(null);
  let importInput: HTMLInputElement | undefined = $state();

  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  const selectedMessage = $derived(
    inbox.find((message) => message.id === selectedMessageId) ?? null,
  );
  const noteOptions = $derived((vault.notes ?? []).slice(0, 40));
  const artifactOptions = $derived((artifacts.artifacts ?? []).slice(0, 40));

  async function refreshNearby() {
    if (!isTauri()) return;
    loadingNearby = true;
    error = null;
    try {
      const response = await discoverLanWorkshops();
      nearby = response.workshops;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingNearby = false;
    }
  }

  async function refreshTrusted() {
    if (!isTauri()) return;
    loadingTrusted = true;
    try {
      trusted = await listTrustedWorkshops();
      const sendable = trusted.filter((entry) => entry.hasSessionToken);
      if (!pushWorkshopId && sendable.length > 0) {
        pushWorkshopId = sendable[0]!.workshopId;
      }
      if (!composeWorkshopId && sendable.length > 0) {
        composeWorkshopId = sendable[0]!.workshopId;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingTrusted = false;
    }
  }

  async function refreshInbox() {
    if (!isTauri()) return;
    loadingInbox = true;
    try {
      inbox = await peerListMessages(false);
      unread = await peerUnreadCount();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingInbox = false;
    }
  }

  async function refreshAll() {
    await Promise.all([refreshNearby(), refreshTrusted(), refreshInbox()]);
  }

  onMount(() => {
    void artifacts.refresh();
    void vault.refreshNotes();
    void refreshAll();
    refreshTimer = setInterval(() => {
      void refreshNearby();
      void refreshInbox();
    }, 5000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  function openTrust(workshop: DiscoveredWorkshop) {
    trustTarget = workshop;
    trustDaemonUrl = workshop.daemonUrl;
    trustQrUrl = "";
    trustName = workshop.peerName ?? workshop.host;
    error = null;
    success = null;
  }

  async function submitTrust() {
    if (!trustDaemonUrl.trim() || !trustQrUrl.trim()) {
      error = "Paste the medousa:// pair link from the other workshop.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = await trustWorkshopFromQr({
        qrUrl: trustQrUrl.trim(),
        daemonUrl: trustDaemonUrl.trim(),
        workshopName: trustName.trim() || null,
      });
      success = `Trusted ${result.workshopPeerName}.`;
      trustTarget = null;
      trustQrUrl = "";
      await workshops.load();
      await refreshTrusted();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function revokeTrust(workshopId: string) {
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
      success = "Share bundle exported.";
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

  async function handlePush() {
    if (!lastBundle) {
      error = "Export a bundle first, then push it.";
      return;
    }
    if (!pushWorkshopId) {
      error = "Choose a trusted workshop.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = await pushShareBundleToWorkshop({
        workshopId: pushWorkshopId,
        bundle: lastBundle,
        conflictStrategy,
      });
      success = `Pushed to peer — ${formatImportResult(result)}`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handleSendMessage() {
    if (!composeWorkshopId) {
      error = "Choose a trusted workshop.";
      return;
    }
    if (!composeBody.trim() && composeAttachKind === "none") {
      error = "Write a message or attach a note/artifact.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      let attachment: Record<string, unknown> | null = null;
      if (composeAttachKind === "note" && composeNotePath) {
        attachment = await exportShareBundle({ vaultPaths: [composeNotePath] });
      } else if (composeAttachKind === "artifact" && composeArtifactId) {
        attachment = await exportShareBundle({ artifactIds: [composeArtifactId] });
      }
      await peerSendMessage({
        workshopId: composeWorkshopId,
        body: composeBody.trim() || "Shared an attachment.",
        attachment,
      });
      composeBody = "";
      composeAttachKind = "none";
      composeNotePath = "";
      composeArtifactId = "";
      success = "Message sent.";
      await refreshInbox();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function openMessage(message: PeerMessage) {
    selectedMessageId = message.id;
    if (!message.readAt) {
      try {
        await peerMarkRead(message.id);
        await refreshInbox();
      } catch {
        /* ignore mark-read failures in UI */
      }
    }
  }

  function formatImportResult(result: ShareImportResult): string {
    return `Imported ${result.surfacesImported} views, ${result.componentsImported} widgets, ${result.vaultNotesImported} notes, ${result.artifactsImported} artifacts.`;
  }

  function isTrusted(workshop: DiscoveredWorkshop): boolean {
    const deviceId = workshop.deviceId;
    if (!deviceId) return false;
    return trusted.some(
      (entry) =>
        entry.workshopDeviceId.startsWith(deviceId.slice(0, 8)) ||
        entry.workshopDeviceId === deviceId,
    );
  }

  function openShareNote() {
    const path = composeNotePath || noteOptions[0]?.path || vault.selectedPath;
    if (!path) {
      error = "Open or pick a vault note first.";
      return;
    }
    shareArtifactId = null;
    shareVaultPath = path;
    shareLabel = path;
    shareSheetOpen = true;
  }

  function openShareArtifact() {
    const id = composeArtifactId || artifactOptions[0]?.artifact_id;
    if (!id) {
      error = "No artifacts available to share.";
      return;
    }
    shareVaultPath = null;
    shareArtifactId = id;
    shareLabel = artifactOptions.find((item) => item.artifact_id === id)?.label ?? id;
    shareSheetOpen = true;
  }

  function formatTime(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">
      Nearby &amp; sharing
      {#if unread > 0}
        <span class="lan-share-unread-pill">{unread}</span>
      {/if}
    </h2>
    <p class="workshop-faint mt-1 text-sm">
      Discover workshops, trust them, share notes and artifacts, and message peers asynchronously.
    </p>
  </header>

  <div class="lan-share-block">
    <div class="lan-share-block-head">
      <h3 class="lan-share-heading">Inbox</h3>
      <button
        type="button"
        class="lan-share-refresh"
        disabled={loadingInbox || busy}
        onclick={() => void refreshInbox()}
      >
        <RefreshCw size={14} class={loadingInbox ? "lan-share-spin" : ""} />
        Refresh
      </button>
    </div>
    {#if inbox.length === 0}
      <p class="lan-share-empty">No peer messages yet.</p>
    {:else}
      <ul class="lan-share-list">
        {#each inbox as message (message.id)}
          <li>
            <button
              type="button"
              class="lan-share-row lan-share-row-btn"
              class:lan-share-row-unread={!message.readAt}
              onclick={() => void openMessage(message)}
            >
              <div class="lan-share-row-copy">
                <p class="lan-share-row-title">{message.fromName}</p>
                <p class="lan-share-row-meta">
                  {message.body.slice(0, 80)}{message.body.length > 80 ? "…" : ""}
                </p>
                <p class="lan-share-row-meta">{formatTime(message.sentAt)}</p>
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if selectedMessage}
      <article class="lan-share-message-detail">
        <header class="lan-share-block-head">
          <h4 class="lan-share-heading">{selectedMessage.fromName}</h4>
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => (selectedMessageId = null)}>
            Close
          </button>
        </header>
        <p class="lan-share-message-body">{selectedMessage.body}</p>
        {#if selectedMessage.attachmentResult}
          <p class="lan-share-row-meta">
            Attachment: {selectedMessage.attachmentResult.summary ??
              (selectedMessage.attachmentResult.imported ? "Imported" : "Not imported")}
          </p>
        {:else if selectedMessage.attachment}
          <p class="lan-share-row-meta">Attachment present (import may have been skipped).</p>
        {/if}
      </article>
    {/if}
  </div>

  <div class="lan-share-block">
    <h3 class="lan-share-heading">Message a peer</h3>
    <p class="lan-share-lead">
      Replies work when both workshops have trusted each other.
    </p>
    {#if trusted.length === 0}
      <p class="lan-share-empty">Trust a workshop below to send messages.</p>
    {:else}
      <label class="lan-share-field">
        <span>To</span>
        <select bind:value={composeWorkshopId} disabled={busy}>
          {#each trusted as workshop (workshop.workshopId)}
            <option value={workshop.workshopId} disabled={!workshop.hasSessionToken}>
              {workshop.label}
            </option>
          {/each}
        </select>
      </label>
      <label class="lan-share-field">
        <span>Message</span>
        <textarea bind:value={composeBody} rows="3" disabled={busy} placeholder="Say hello…"></textarea>
      </label>
      <label class="lan-share-field">
        <span>Attachment</span>
        <select bind:value={composeAttachKind} disabled={busy}>
          <option value="none">None</option>
          <option value="note">Vault note</option>
          <option value="artifact">Artifact</option>
        </select>
      </label>
      {#if composeAttachKind === "note"}
        <label class="lan-share-field">
          <span>Note path</span>
          <select bind:value={composeNotePath} disabled={busy}>
            <option value="">Select note…</option>
            {#each noteOptions as note (note.path)}
              <option value={note.path}>{note.path}</option>
            {/each}
          </select>
        </label>
      {:else if composeAttachKind === "artifact"}
        <label class="lan-share-field">
          <span>Artifact</span>
          <select bind:value={composeArtifactId} disabled={busy}>
            <option value="">Select artifact…</option>
            {#each artifactOptions as item (item.artifact_id)}
              <option value={item.artifact_id}>{item.label ?? item.artifact_id}</option>
            {/each}
          </select>
        </label>
      {/if}
      <button
        type="button"
        class="btn btn-sm btn-primary"
        disabled={busy}
        onclick={() => void handleSendMessage()}
      >
        Send message
      </button>
    {/if}
  </div>

  <div class="lan-share-block">
    <div class="lan-share-block-head">
      <h3 class="lan-share-heading">Nearby workshops</h3>
      <button
        type="button"
        class="lan-share-refresh"
        disabled={loadingNearby || busy}
        onclick={() => void refreshNearby()}
      >
        <RefreshCw size={14} class={loadingNearby ? "lan-share-spin" : ""} />
        Refresh
      </button>
    </div>
    {#if nearby.length === 0}
      <p class="lan-share-empty">No other workshops found on your network right now.</p>
    {:else}
      <ul class="lan-share-list">
        {#each nearby as workshop (workshop.daemonUrl)}
          <li class="lan-share-row">
            <div class="lan-share-row-copy">
              <p class="lan-share-row-title">{workshop.peerName ?? workshop.host}</p>
              <p class="lan-share-row-meta">
                {workshop.deviceId?.slice(0, 8) ?? "unknown"} · {workshop.daemonUrl}
              </p>
              {#if capabilityBadges(workshop.capabilityFlags).length > 0}
                <div class="lan-share-badges">
                  {#each capabilityBadges(workshop.capabilityFlags) as badge (badge)}
                    <span class="lan-share-badge">{badge}</span>
                  {/each}
                </div>
              {/if}
            </div>
            <div class="lan-share-row-actions">
              {#if isTrusted(workshop)}
                <span class="lan-share-trusted-label">Trusted</span>
              {:else}
                <button
                  type="button"
                  class="btn btn-sm btn-ghost"
                  disabled={busy}
                  onclick={() => openTrust(workshop)}
                >
                  Trust
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if trustTarget}
    <div class="lan-share-trust-panel">
      <h3 class="lan-share-heading">Trust {trustTarget.peerName ?? trustTarget.host}</h3>
      <p class="lan-share-lead">
        On the other workshop, open Settings → Nearby and copy its pair link, or paste its QR URL
        below.
      </p>
      <label class="lan-share-field">
        <span>Pair link</span>
        <input
          type="text"
          bind:value={trustQrUrl}
          placeholder="medousa://pair/1.0?..."
          disabled={busy}
        />
      </label>
      <label class="lan-share-field">
        <span>Workshop URL</span>
        <input type="text" bind:value={trustDaemonUrl} disabled={busy} />
      </label>
      <label class="lan-share-field">
        <span>Display name</span>
        <input type="text" bind:value={trustName} disabled={busy} />
      </label>
      <div class="lan-share-actions">
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={busy}
          onclick={() => void submitTrust()}
        >
          {busy ? "Trusting…" : "Trust workshop"}
        </button>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          disabled={busy}
          onclick={() => (trustTarget = null)}
        >
          Cancel
        </button>
      </div>
    </div>
  {/if}

  <div class="lan-share-block">
    <h3 class="lan-share-heading">Trusted workshops</h3>
    {#if trusted.length === 0}
      <p class="lan-share-empty">Trust a nearby workshop to share and message.</p>
    {:else}
      <ul class="lan-share-list">
        {#each trusted as workshop (workshop.workshopId)}
          <li class="lan-share-row">
            <div class="lan-share-row-copy">
              <p class="lan-share-row-title">{workshop.label}</p>
              <p class="lan-share-row-meta">
                {workshop.daemonUrl}
                {#if workshop.hasSessionToken}
                  · connected
                {:else}
                  · needs re-trust
                {/if}
              </p>
            </div>
            <button
              type="button"
              class="btn btn-sm btn-ghost"
              disabled={busy}
              onclick={() => void revokeTrust(workshop.workshopId)}
            >
              Revoke
            </button>
          </li>
        {/each}
      </ul>
      <div class="lan-share-actions mt-2">
        <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={openShareNote}>
          Share a note…
        </button>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          disabled={busy}
          onclick={openShareArtifact}
        >
          Share an artifact…
        </button>
      </div>
    {/if}
  </div>

  <div class="lan-share-block">
    <h3 class="lan-share-heading">Share bundle</h3>
    <p class="lan-share-lead">
      Export custom canvas views as a bundle — import on this machine or push to a trusted peer.
    </p>
    <label class="lan-share-checkbox">
      <input type="checkbox" bind:checked={includeEnvironment} disabled={busy} />
      Include custom canvas views and widgets
    </label>
    <label class="lan-share-field">
      <span>Import conflicts</span>
      <select bind:value={conflictStrategy} disabled={busy}>
        <option value="rename">Rename duplicates</option>
        <option value="skip">Skip duplicates</option>
        <option value="overwrite">Overwrite duplicates</option>
      </select>
    </label>
    <div class="lan-share-actions">
      <button
        type="button"
        class="btn btn-sm btn-primary"
        disabled={busy}
        onclick={() => void handleExport()}
      >
        <Share2 size={14} />
        Export bundle
      </button>
      <button
        type="button"
        class="btn btn-sm btn-ghost"
        disabled={busy}
        onclick={() => importInput?.click()}
      >
        <Upload size={14} />
        Import file
      </button>
      <input
        bind:this={importInput}
        type="file"
        accept="application/json,.json"
        class="hidden"
        onchange={handleImportFile}
      />
    </div>
    {#if trusted.length > 0}
      <div class="lan-share-push">
        <label class="lan-share-field">
          <span>Push to trusted workshop</span>
          <select bind:value={pushWorkshopId} disabled={busy}>
            {#each trusted as workshop (workshop.workshopId)}
              <option value={workshop.workshopId}>{workshop.label}</option>
            {/each}
          </select>
        </label>
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={busy || !lastBundle}
          onclick={() => void handlePush()}
        >
          <ShieldCheck size={14} />
          Push bundle
        </button>
      </div>
    {/if}
  </div>

  {#if error}
    <p class="lan-share-error">{error}</p>
  {/if}
  {#if success}
    <p class="lan-share-success">{success}</p>
  {/if}
</section>

<ShareToPeerSheet
  open={shareSheetOpen}
  artifactId={shareArtifactId}
  vaultPath={shareVaultPath}
  label={shareLabel}
  onClose={() => {
    shareSheetOpen = false;
  }}
  onShared={(message) => {
    success = message;
  }}
  onError={(message) => {
    error = message;
  }}
/>

<style>
  .lan-share-block {
    margin-top: 1.25rem;
  }

  .lan-share-block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.55rem;
  }

  .lan-share-heading {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-unread-pill {
    display: inline-flex;
    margin-left: 0.4rem;
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
    font-size: 0.6875rem;
    font-weight: 700;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    vertical-align: middle;
  }

  .lan-share-lead,
  .lan-share-empty {
    margin: 0 0 0.65rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .lan-share-refresh {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    background: transparent;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .lan-share-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.45rem;
  }

  .lan-share-row,
  .lan-share-row-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    width: 100%;
    padding: 0.65rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
    text-align: left;
  }

  .lan-share-row-btn {
    cursor: pointer;
    color: inherit;
  }

  .lan-share-row-unread {
    border-color: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
  }

  .lan-share-row-copy {
    min-width: 0;
  }

  .lan-share-row-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-row-meta {
    margin: 0.15rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-500));
    overflow-wrap: anywhere;
  }

  .lan-share-message-detail {
    margin-top: 0.75rem;
    padding: 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 50%, transparent);
  }

  .lan-share-message-body {
    margin: 0.35rem 0;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-200));
    white-space: pre-wrap;
  }

  .lan-share-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    margin-top: 0.35rem;
  }

  .lan-share-badge {
    border-radius: 999px;
    padding: 0.1rem 0.45rem;
    font-size: 0.625rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .lan-share-trusted-label {
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-success-300));
  }

  .lan-share-trust-panel {
    margin-top: 1rem;
    padding: 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 8%, transparent);
  }

  .lan-share-field {
    display: grid;
    gap: 0.25rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
  }

  .lan-share-field span {
    color: rgb(var(--color-surface-400));
  }

  .lan-share-field input,
  .lan-share-field select,
  .lan-share-field textarea {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-checkbox {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
  }

  .lan-share-actions,
  .lan-share-push {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.45rem;
  }

  .lan-share-push {
    margin-top: 0.75rem;
  }

  .lan-share-error {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .lan-share-success {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-success-300));
  }

  :global(.lan-share-spin) {
    animation: lan-share-spin 900ms linear infinite;
  }

  @keyframes lan-share-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
