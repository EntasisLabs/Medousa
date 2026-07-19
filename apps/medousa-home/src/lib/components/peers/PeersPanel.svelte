<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import PeerComposer from "$lib/components/peers/PeerComposer.svelte";
  import PeerListRow from "$lib/components/peers/PeerListRow.svelte";
  import PeerThread from "$lib/components/peers/PeerThread.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { peersShell } from "$lib/stores/peersShell.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    connectToNearbyWorkshop,
    discoverLanWorkshops,
    exportShareBundle,
    getLanPairingStatus,
    listTrustedWorkshops,
    peerListMessages,
    peerMarkThreadRead,
    peerComposeIdentity,
    peerSendMessage,
    peerUnreadCount,
    revokeTrustedWorkshop,
    setLanPairingEnabled,
    trustWorkshopFromQr,
    type DiscoveredWorkshop,
    type LanPairingStatus,
    type PeerComposeIdentity,
    type PeerMessage,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import {
    fetchBonjourStatus,
    fetchPairingQr,
    fetchPairingQrImage,
    formatCountdown,
    formatShortCode,
    rotatePairingInvite,
    secondsUntil,
    type BonjourStatus,
    type PairingQrImage,
  } from "$lib/utils/pairingApi";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { consumePendingPeerNavigation } from "$lib/peerNavigation";
  import {
    deviceIdsMatch,
    matchesPeer,
    sortPeerConversations,
  } from "$lib/utils/peerHomePreview";
  import { isTauri } from "$lib/window";
  import {
    Copy,
    Link2,
    Plus,
    RefreshCw,
    Search,
    Users,
    Wifi,
    X,
  } from "@lucide/svelte";

  interface Props {
    visible?: boolean;
    /** Single-column list↔thread stack for phone. */
    mobile?: boolean;
    /** Nested under More hub (no duplicate page title). */
    embedded?: boolean;
  }

  let { visible = true, mobile = false, embedded = false }: Props = $props();

  let qr = $state<PairingQrImage | null>(null);
  let countdown = $state(0);
  let bonjour = $state<BonjourStatus | null>(null);
  let nearby = $state<DiscoveredWorkshop[]>([]);
  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let inbox = $state<PeerMessage[]>([]);
  let unread = $state(0);
  let selectedPeerId = $state<string | null>(null);
  let composeBody = $state("");
  let composeAttachKind = $state<"none" | "note" | "artifact">("none");
  let composeNotePath = $state("");
  let composeArtifactId = $state("");
  let attachMenuOpen = $state(false);
  let peerMenuOpen = $state(false);
  let renaming = $state(false);
  let renameDraft = $state("");
  let addPeerOpen = $state(false);
  let fallbackOpen = $state(false);
  let fallbackUrlInputEl = $state<HTMLInputElement | null>(null);
  let fallbackDaemonUrl = $state("");
  let fallbackQrUrl = $state("");
  let fallbackName = $state("");
  let busy = $state(false);
  let connectingUrl = $state<string | null>(null);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let copyFlash = $state(false);
  let lanPairing = $state<LanPairingStatus | null>(null);
  let lanBusy = $state(false);
  let composeIdentity = $state<PeerComposeIdentity | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let countdownTimer: ReturnType<typeof setInterval> | null = null;

  const shellList = $derived(!mobile && !embedded);
  const peopleQuery = $derived(peersShell.peopleQuery);
  const noteOptions = $derived((vault.notes ?? []).slice(0, 40));
  const artifactOptions = $derived((artifacts.artifacts ?? []).slice(0, 40));
  const hasPeers = $derived(trusted.length > 0);
  const showPeopleSearch = $derived(trusted.length >= 5);
  const nearbyUntrusted = $derived(nearby.filter((workshop) => !isTrustedNearby(workshop)));

  const conversationRows = $derived.by(() => {
    const nearbyIds = nearby
      .map((workshop) => workshop.deviceId)
      .filter((id): id is string => Boolean(id));
    let rows = sortPeerConversations(trusted, inbox, nearbyIds);
    const needle = peopleQuery.trim().toLowerCase();
    if (needle) {
      rows = rows.filter(
        (row) =>
          row.label.toLowerCase().includes(needle) ||
          row.peer.workshopDeviceId.toLowerCase().includes(needle),
      );
    }
    return rows;
  });

  $effect(() => {
    peersShell.publish({
      rows: conversationRows,
      nearbyUntrustedCount: nearbyUntrusted.length,
      hasPeers,
      showPeopleSearch,
      selectedPeerId,
    });
  });

  const selectedPeer = $derived(
    trusted.find((entry) => entry.workshopId === selectedPeerId) ?? null,
  );

  const threadMessages = $derived.by(() => {
    if (!selectedPeer) return [];
    return inbox.filter((message) => matchesPeer(message, selectedPeer.workshopDeviceId));
  });

  const composeAsLabel = $derived.by(() => {
    if (!selectedPeer) return "";
    if (selectedPeer.inbound) {
      return `Replying as ${composeIdentity?.workshopName ?? "workshop"}`;
    }
    return `Sending as ${composeIdentity?.clientName ?? "you"}`;
  });

  const canCompose = $derived(
    Boolean(selectedPeer && (selectedPeer.inbound || selectedPeer.hasSessionToken)),
  );

  function isTrustedNearby(workshop: DiscoveredWorkshop): boolean {
    const deviceId = workshop.deviceId;
    if (!deviceId) return false;
    return trusted.some(
      (entry) =>
        entry.workshopDeviceId === deviceId ||
        entry.workshopDeviceId.startsWith(deviceId.slice(0, 8)),
    );
  }

  function findNearbyForPeer(peer: TrustedWorkshopSummary): DiscoveredWorkshop | null {
    return (
      nearby.find(
        (workshop) =>
          workshop.deviceId && deviceIdsMatch(workshop.deviceId, peer.workshopDeviceId),
      ) ?? null
    );
  }

  async function refreshInvite() {
    if (!isTauri()) return;
    try {
      qr = await fetchPairingQrImage();
      countdown = secondsUntil(qr.expiresAt);
      bonjour = await fetchBonjourStatus();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function rotateInvite() {
    busy = true;
    error = null;
    try {
      await rotatePairingInvite();
      await refreshInvite();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function copyInviteLink(full = false) {
    try {
      const invite = full ? await fetchPairingQr({ full: true }) : qr;
      const url = invite?.url;
      if (!url) return;
      await navigator.clipboard.writeText(url);
      copyFlash = true;
      setTimeout(() => {
        copyFlash = false;
      }, 1500);
      success = full
        ? "Full invite copied (works off-LAN when pasted)."
        : "Invite copied — scan or open on the same Wi‑Fi.";
    } catch {
      error = "Could not copy invite link.";
    }
  }

  async function refreshNearby() {
    if (!isTauri()) return;
    try {
      const response = await discoverLanWorkshops();
      nearby = response.workshops;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshTrusted() {
    if (!isTauri()) return;
    try {
      trusted = await listTrustedWorkshops();
      const pending = consumePendingPeerNavigation();
      if (pending && trusted.some((peer) => peer.workshopId === pending)) {
        selectedPeerId = pending;
      } else if (!selectedPeerId && trusted.length > 0 && !mobile) {
        selectedPeerId = trusted[0]!.workshopId;
      }
      if (selectedPeerId && !trusted.some((peer) => peer.workshopId === selectedPeerId)) {
        selectedPeerId = trusted[0]?.workshopId ?? null;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshInbox() {
    if (!isTauri()) return;
    try {
      inbox = await peerListMessages(false);
      unread = await peerUnreadCount();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshComposeIdentity() {
    if (!isTauri()) return;
    try {
      composeIdentity = await peerComposeIdentity();
    } catch {
      composeIdentity = null;
    }
  }

  async function refreshLanPairing() {
    if (!isTauri()) return;
    try {
      lanPairing = await getLanPairingStatus();
    } catch {
      /* optional */
    }
  }

  async function toggleLanPairing(enabled: boolean) {
    lanBusy = true;
    error = null;
    try {
      lanPairing = await setLanPairingEnabled(enabled);
      success = lanPairing.message;
      await refreshInvite();
      await refreshNearby();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      await refreshLanPairing();
    } finally {
      lanBusy = false;
    }
  }

  async function refreshAll() {
    const tasks: Array<Promise<unknown>> = [
      refreshNearby(),
      refreshTrusted(),
      refreshInbox(),
      refreshComposeIdentity(),
    ];
    if (!mobile) {
      tasks.push(refreshInvite(), refreshLanPairing());
    }
    await Promise.all(tasks);
  }

  async function connectNearby(workshop: DiscoveredWorkshop) {
    connectingUrl = workshop.daemonUrl;
    error = null;
    success = null;
    busy = true;
    try {
      const result = await connectToNearbyWorkshop({
        daemonUrl: workshop.daemonUrl,
        peerName: workshop.peerName ?? workshop.host,
      });
      success = `Connected to ${result.workshopPeerName}. Ask them to Connect back so they can reply.`;
      addPeerOpen = false;
      await workshops.load();
      await refreshTrusted();
      selectedPeerId = result.workshopId;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      fallbackDaemonUrl = workshop.daemonUrl;
      fallbackName = workshop.peerName ?? workshop.host;
      fallbackQrUrl = "";
      fallbackOpen = true;
    } finally {
      busy = false;
      connectingUrl = null;
    }
  }

  function daemonUrlFromInvite(qrUrl: string): string | null {
    try {
      const url = new URL(qrUrl);
      const address = url.searchParams.get("a")?.trim();
      if (!address) return null;
      if (address.startsWith("http://") || address.startsWith("https://")) {
        return address.replace(/\/$/, "");
      }
      return `http://${address}`;
    } catch {
      return null;
    }
  }

  async function submitFallback() {
    const daemonUrl = fallbackDaemonUrl.trim();
    const qrUrl = fallbackQrUrl.trim();
    if (!daemonUrl && !qrUrl) {
      error = "Enter a workshop URL (e.g. http://10.12.0.13:7419).";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = qrUrl
        ? await trustWorkshopFromQr({
            qrUrl,
            daemonUrl: daemonUrl || daemonUrlFromInvite(qrUrl) || "",
            workshopName: fallbackName.trim() || null,
          })
        : await connectToNearbyWorkshop({
            daemonUrl,
            peerName: fallbackName.trim() || null,
          });
      success = `Connected to ${result.workshopPeerName}. Ask them to Connect back so they can reply.`;
      fallbackOpen = false;
      addPeerOpen = false;
      await workshops.load();
      await refreshTrusted();
      selectedPeerId = result.workshopId;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function reconnectSelected() {
    if (!selectedPeer || selectedPeer.inbound) return;
    error = null;
    success = null;
    await refreshNearby();
    const match = findNearbyForPeer(selectedPeer);
    if (match) {
      await connectNearby(match);
      return;
    }
    fallbackDaemonUrl = selectedPeer.daemonUrl || "";
    fallbackName = selectedPeer.label;
    fallbackQrUrl = "";
    fallbackOpen = true;
  }

  function startRename() {
    if (!selectedPeer || selectedPeer.inbound) return;
    peerMenuOpen = false;
    renameDraft = selectedPeer.label;
    renaming = true;
  }

  function cancelRename() {
    renaming = false;
    renameDraft = "";
  }

  async function commitRename() {
    if (!selectedPeerId) {
      cancelRename();
      return;
    }
    const label = renameDraft.trim();
    if (!label) {
      cancelRename();
      return;
    }
    busy = true;
    error = null;
    try {
      await workshops.renameWorkshop(selectedPeerId, label);
      await refreshTrusted();
      renaming = false;
      renameDraft = "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function revokePeer(workshopId: string) {
    peerMenuOpen = false;
    busy = true;
    try {
      await revokeTrustedWorkshop(workshopId);
      if (selectedPeerId === workshopId) selectedPeerId = null;
      await workshops.load();
      await refreshTrusted();
      success = "Peer removed.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function sendMessage() {
    if (!selectedPeerId) {
      error = "Choose a peer.";
      return;
    }
    if (!composeBody.trim() && composeAttachKind === "none") {
      error = "Write a message or attach something.";
      return;
    }
    busy = true;
    error = null;
    try {
      let attachment: Record<string, unknown> | null = null;
      if (composeAttachKind === "note" && composeNotePath) {
        attachment = await exportShareBundle({ vaultPaths: [composeNotePath] });
      } else if (composeAttachKind === "artifact" && composeArtifactId) {
        attachment = await exportShareBundle({ artifactIds: [composeArtifactId] });
      }
      await peerSendMessage({
        workshopId: selectedPeerId,
        body: composeBody.trim() || "Shared an attachment.",
        attachment,
      });
      composeBody = "";
      clearAttachment();
      success = null;
      await refreshInbox();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function clearAttachment() {
    composeAttachKind = "none";
    composeNotePath = "";
    composeArtifactId = "";
    attachMenuOpen = false;
  }

  async function markThreadRead() {
    if (!selectedPeer) return;
    peerMenuOpen = false;
    try {
      await peerMarkThreadRead(selectedPeer.workshopDeviceId);
      await refreshInbox();
    } catch (err) {
      toast.show(err instanceof Error ? err.message : String(err));
    }
  }

  async function selectPeer(workshopId: string) {
    selectedPeerId = workshopId;
    peerMenuOpen = false;
    attachMenuOpen = false;
    cancelRename();
    const peer = trusted.find((entry) => entry.workshopId === workshopId);
    if (peer) {
      try {
        await peerMarkThreadRead(peer.workshopDeviceId);
        await refreshInbox();
      } catch {
        /* best-effort on open */
      }
    }
  }

  function openAddPeer() {
    error = null;
    success = null;
    addPeerOpen = true;
    if (!mobile) {
      void refreshInvite();
    }
    void refreshNearby();
  }

  $effect(() => {
    peersShell.onSelectPeer = (id) => {
      void selectPeer(id);
    };
    peersShell.onAddPeer = () => openAddPeer();
    return () => {
      peersShell.onSelectPeer = null;
      peersShell.onAddPeer = null;
    };
  });

  $effect(() => {
    if (!visible) return;
    void refreshAll();
  });

  onMount(() => {
    void artifacts.refresh();
    void vault.refreshNotes();
    void refreshAll();
    pollTimer = setInterval(() => {
      if (!visible) return;
      void refreshNearby();
      void refreshInbox();
      if (qr && secondsUntil(qr.expiresAt) <= 0) {
        void refreshInvite();
      }
    }, 5000);
    countdownTimer = setInterval(() => {
      if (qr) countdown = secondsUntil(qr.expiresAt);
    }, 1000);
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (countdownTimer) clearInterval(countdownTimer);
  });

  $effect(() => {
    if (!mobile || !visible) return;
    return registerMobileBackHandler(() => {
      if (addPeerOpen) {
        addPeerOpen = false;
        return true;
      }
      if (fallbackOpen) {
        fallbackOpen = false;
        return true;
      }
      if (selectedPeerId) {
        selectedPeerId = null;
        peerMenuOpen = false;
        cancelRename();
        return true;
      }
      return false;
    });
  });

  $effect(() => {
    if (fallbackOpen) {
      fallbackUrlInputEl?.focus();
    }
  });
</script>

<section
  class="peers-panel {visible ? '' : 'hidden'}"
  class:peers-panel-mobile={mobile}
  class:peers-panel-embedded={embedded}
  aria-label="Peers"
>
  {#if !shellList}
  <aside class="peers-sidebar" class:peers-mobile-pane-hidden={mobile && !!selectedPeer}>
    <header class="peers-sidebar-head">
      {#if !embedded}
        <div>
          <h1 class="peers-app-title">Peers</h1>
          <p class="peers-app-sub">People on your network</p>
        </div>
      {:else}
        <div>
          <p class="peers-app-sub">Connect and message other workshops</p>
        </div>
      {/if}
      <button
        type="button"
        class="peers-add-btn"
        title="Add peer"
        aria-label="Add peer"
        disabled={busy}
        onclick={openAddPeer}
      >
        <Plus size={18} strokeWidth={2} />
      </button>
    </header>

    {#if nearbyUntrusted.length > 0}
      <div class="peers-nearby-banner" role="status">
        <Wifi size={14} strokeWidth={2} />
        <div class="peers-nearby-banner-copy">
          <p class="peers-nearby-banner-title">
            {nearbyUntrusted[0]!.peerName ?? nearbyUntrusted[0]!.host} is nearby
          </p>
          <p class="peers-nearby-banner-meta">Same Wi‑Fi — tap Connect</p>
        </div>
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={busy}
          onclick={() => void connectNearby(nearbyUntrusted[0]!)}
        >
          {connectingUrl === nearbyUntrusted[0]!.daemonUrl ? "…" : "Connect"}
        </button>
      </div>
      {#if nearbyUntrusted.length > 1}
        <div class="peers-nearby-more">
          {#each nearbyUntrusted.slice(1) as workshop (workshop.daemonUrl)}
            <button
              type="button"
              class="peers-nearby-chip"
              disabled={busy}
              onclick={() => void connectNearby(workshop)}
            >
              {workshop.peerName ?? workshop.host}
            </button>
          {/each}
        </div>
      {/if}
    {/if}

    {#if showPeopleSearch}
      <label class="peers-sidebar-search">
        <Search size={14} strokeWidth={1.75} class="peers-sidebar-search-icon" aria-hidden="true" />
        <input
          class="peers-sidebar-search-input"
          type="search"
          placeholder="Search people…"
          value={peopleQuery}
          oninput={(event) => {
            peersShell.peopleQuery = (event.currentTarget as HTMLInputElement).value;
          }}
        />
      </label>
    {/if}

    {#if !hasPeers}
      <div class="peers-empty-people">
        <Users size={22} strokeWidth={1.5} />
        <p>Add someone nearby</p>
        <button type="button" class="btn btn-sm btn-primary" onclick={openAddPeer}>
          Add peer
        </button>
      </div>
    {:else if conversationRows.length === 0}
      <div class="peers-empty-people">
        <p>No people match “{peopleQuery.trim()}”.</p>
      </div>
    {:else}
      <ul class="peers-people peers-people-scroll">
        {#each conversationRows as row (row.workshopId)}
          <li>
            <PeerListRow
              {row}
              selected={selectedPeerId === row.workshopId}
              onSelect={() => void selectPeer(row.workshopId)}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </aside>
  {/if}

  <div class="peers-main" class:peers-mobile-pane-hidden={mobile && !selectedPeer}>
    {#if shellList}
      <div class="flex items-center gap-2 border-b border-surface-500/35 px-3 py-2">
        <ShellSidebarExpandButton label="Show people" />
        <p class="text-sm font-semibold text-surface-100">Peers</p>
      </div>
    {/if}
    {#if !selectedPeer}
      <div class="peers-empty-main">
        <div class="peers-empty-icon" aria-hidden="true">
          <Users size={28} strokeWidth={1.5} />
        </div>
        <h2>{hasPeers ? "Choose someone" : "Add someone nearby"}</h2>
        <p>
          {#if hasPeers}
            Pick a person to message, or add another with +.
          {:else if nearbyUntrusted.length > 0}
            Someone is on this Wi‑Fi — tap Connect, or show your invite.
          {:else}
            Show your invite or connect to someone on the same Wi‑Fi.
          {/if}
        </p>
        {#if !hasPeers}
          <button type="button" class="btn btn-sm btn-primary" onclick={openAddPeer}>
            {mobile ? "Connect" : "Show invite"}
          </button>
        {/if}
        {#if unread > 0}
          <p class="peers-unread-banner">{unread} unread</p>
        {/if}
      </div>
    {:else}
      <PeerThread
        peer={selectedPeer}
        messages={threadMessages}
        identityLabel={composeAsLabel}
        {busy}
        {renaming}
        {renameDraft}
        menuOpen={peerMenuOpen}
        onToggleMenu={() => (peerMenuOpen = !peerMenuOpen)}
        onMarkRead={() => void markThreadRead()}
        onStartRename={startRename}
        onCommitRename={() => void commitRename()}
        onCancelRename={cancelRename}
        onRenameDraftChange={(value) => (renameDraft = value)}
        onRemove={() => void revokePeer(selectedPeer.workshopId)}
        onReconnect={() => void reconnectSelected()}
      />

      <PeerComposer
        peerLabel={selectedPeer.label}
        bind:body={composeBody}
        {busy}
        canSend={canCompose}
        needsReconnect={!selectedPeer.inbound && !selectedPeer.hasSessionToken}
        attachKind={composeAttachKind}
        notePath={composeNotePath}
        artifactId={composeArtifactId}
        {noteOptions}
        {artifactOptions}
        {attachMenuOpen}
        onToggleAttachMenu={() => (attachMenuOpen = !attachMenuOpen)}
        onPickNote={(path) => {
          composeAttachKind = "note";
          composeNotePath = path;
          attachMenuOpen = false;
        }}
        onPickArtifact={(id) => {
          composeAttachKind = "artifact";
          composeArtifactId = id;
          attachMenuOpen = false;
        }}
        onClearAttachment={clearAttachment}
        onSend={() => void sendMessage()}
        onReconnect={() => void reconnectSelected()}
      />
    {/if}

    {#if !mobile && error}
      <p class="peers-error">{error}</p>
    {/if}
    {#if !mobile && success}
      <p class="peers-success">{success}</p>
    {/if}
  </div>

  {#if mobile && (error || success)}
    <div class="peers-mobile-status">
      {#if error}
        <p class="peers-error">{error}</p>
      {/if}
      {#if success}
        <p class="peers-success">{success}</p>
      {/if}
    </div>
  {/if}
</section>

{#if addPeerOpen}
  <div
    class="peers-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) addPeerOpen = false;
    }}
  >
    <div class="peers-sheet" role="dialog" aria-modal="true" aria-label="Add peer">
      <header class="peers-sheet-head">
        <h2>Add peer</h2>
        <button type="button" class="peers-icon-btn" aria-label="Close" onclick={() => (addPeerOpen = false)}>
          <X size={18} />
        </button>
      </header>
      {#if mobile}
        <p class="peers-sheet-lead">
          Connect to someone on your Wi‑Fi by address, or tap Connect when they appear nearby.
        </p>
      {:else}
        <p class="peers-sheet-lead">
          Others on your Wi‑Fi can tap Connect on your name — or scan this invite.
        </p>

        <label class="peers-lan-toggle">
          <input
            type="checkbox"
            checked={lanPairing?.enabled ?? false}
            disabled={lanBusy}
            onchange={(event) =>
              void toggleLanPairing((event.currentTarget as HTMLInputElement).checked)}
          />
          <span>LAN pairing window (restarts engine)</span>
        </label>
        {#if lanPairing}
          <p class="peers-sheet-lead">{lanPairing.message}</p>
        {/if}

        {#if bonjour?.likelyAdvertising}
          <span class="peers-visible-pill">Visible on network</span>
        {/if}

        {#if qr}
          <img class="peers-qr" src={qr.dataUrl} alt="Peer invite QR code" />
          <p class="peers-code">{formatShortCode(qr.shortCode)}</p>
          <p class="peers-countdown">Expires in {formatCountdown(countdown)}</p>
          <p class="peers-sheet-hint">Camera scan works on the same Wi‑Fi. Paste full link only if they are off-LAN.</p>
          <div class="peers-sheet-actions">
            <button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={() => void copyInviteLink(false)}>
              <Copy size={14} />
              {copyFlash ? "Copied" : "Copy link"}
            </button>
            <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={() => void copyInviteLink(true)}>
              Full link
            </button>
            <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={() => void rotateInvite()}>
              <RefreshCw size={14} />
              Refresh
            </button>
          </div>
        {:else}
          <p class="peers-sheet-lead">Loading invite…</p>
        {/if}
      {/if}

      {#if nearbyUntrusted.length > 0}
        <div class="peers-sheet-nearby">
          <h3>Nearby</h3>
          <ul class="peers-sheet-nearby-list">
            {#each nearbyUntrusted as workshop (workshop.daemonUrl)}
              <li class="peers-sheet-nearby-row">
                <span>{workshop.peerName ?? workshop.host}</span>
                <button
                  type="button"
                  class="btn btn-sm btn-primary"
                  disabled={busy}
                  onclick={() => void connectNearby(workshop)}
                >
                  {connectingUrl === workshop.daemonUrl ? "…" : "Connect"}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <button
        type="button"
        class="peers-paste-link"
        onclick={() => {
          fallbackDaemonUrl = "";
          fallbackQrUrl = "";
          fallbackName = "";
          fallbackOpen = true;
        }}
      >
        <Link2 size={14} />
        Connect by address
      </button>
    </div>
  </div>
{/if}

{#if fallbackOpen}
  <div
    class="peers-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) fallbackOpen = false;
    }}
  >
    <div class="peers-sheet peers-sheet-sm" role="dialog" aria-modal="true" aria-label="Connect by address">
      <header class="peers-sheet-head">
        <h2>Connect by address</h2>
        <button type="button" class="peers-icon-btn" aria-label="Close" onclick={() => (fallbackOpen = false)}>
          <X size={18} />
        </button>
      </header>
      <p class="peers-sheet-hint">
        Same as <code>medousa peer connect</code> — we fetch their invite over the LAN.
      </p>
      <label class="peers-field">
        <span>Workshop URL</span>
        <input
          bind:this={fallbackUrlInputEl}
          type="text"
          bind:value={fallbackDaemonUrl}
          placeholder="http://10.12.0.13:7419"
          disabled={busy}
        />
      </label>
      <label class="peers-field">
        <span>Name <span class="peers-field-optional">optional</span></span>
        <input type="text" bind:value={fallbackName} placeholder="Their workshop" disabled={busy} />
      </label>
      <label class="peers-field">
        <span>Invite link <span class="peers-field-optional">optional</span></span>
        <input
          type="text"
          bind:value={fallbackQrUrl}
          placeholder="medousa://pair/… only if /qr is unreachable"
          disabled={busy}
        />
      </label>
      <div class="peers-sheet-actions">
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={busy || (!fallbackDaemonUrl.trim() && !fallbackQrUrl.trim())}
          onclick={() => void submitFallback()}
        >
          {busy ? "Connecting…" : "Connect"}
        </button>
      </div>
    </div>
  </div>
{/if}
