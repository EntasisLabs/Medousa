<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    connectToNearbyWorkshop,
    discoverLanWorkshops,
    exportShareBundle,
    getLanPairingStatus,
    listTrustedWorkshops,
    peerListMessages,
    peerMarkRead,
    peerSendMessage,
    peerUnreadCount,
    revokeTrustedWorkshop,
    setLanPairingEnabled,
    trustWorkshopFromQr,
    type DiscoveredWorkshop,
    type LanPairingStatus,
    type PeerMessage,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import {
    fetchBonjourStatus,
    fetchPairingQrImage,
    formatCountdown,
    formatShortCode,
    rotatePairingInvite,
    secondsUntil,
    type BonjourStatus,
    type PairingQrImage,
  } from "$lib/utils/pairingApi";
  import { isTauri } from "$lib/window";
  import {
    Copy,
    Link2,
    MoreHorizontal,
    Paperclip,
    Plus,
    RefreshCw,
    Send,
    Users,
    Wifi,
    X,
  } from "@lucide/svelte";

  interface Props {
    visible?: boolean;
  }

  let { visible = true }: Props = $props();

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

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let countdownTimer: ReturnType<typeof setInterval> | null = null;

  const noteOptions = $derived((vault.notes ?? []).slice(0, 40));
  const artifactOptions = $derived((artifacts.artifacts ?? []).slice(0, 40));
  const hasPeers = $derived(trusted.length > 0);
  const nearbyUntrusted = $derived(nearby.filter((workshop) => !isTrustedNearby(workshop)));

  const selectedPeer = $derived(
    trusted.find((entry) => entry.workshopId === selectedPeerId) ?? null,
  );

  const threadMessages = $derived.by(() => {
    if (!selectedPeer) return [];
    const deviceId = selectedPeer.workshopDeviceId;
    return inbox.filter((message) => matchesPeer(message, deviceId));
  });

  const attachmentLabel = $derived.by(() => {
    if (composeAttachKind === "note" && composeNotePath) return composeNotePath;
    if (composeAttachKind === "artifact" && composeArtifactId) {
      return (
        artifactOptions.find((item) => item.artifact_id === composeArtifactId)?.label ??
        composeArtifactId
      );
    }
    return null;
  });

  function deviceIdsMatch(left: string, right: string): boolean {
    if (!left || !right) return left === right;
    return (
      left === right ||
      left.startsWith(right.slice(0, 8)) ||
      right.startsWith(left.slice(0, 8))
    );
  }

  function matchesPeer(message: PeerMessage, deviceId: string): boolean {
    if (deviceIdsMatch(message.fromDeviceId, deviceId)) return true;
    if (message.toDeviceId && deviceIdsMatch(message.toDeviceId, deviceId)) return true;
    return false;
  }

  function isOutbound(message: PeerMessage): boolean {
    return message.direction === "out";
  }

  function unreadForPeer(peer: TrustedWorkshopSummary): number {
    return inbox.filter(
      (message) =>
        !isOutbound(message) && !message.readAt && matchesPeer(message, peer.workshopDeviceId),
    ).length;
  }

  function isTrustedNearby(workshop: DiscoveredWorkshop): boolean {
    const deviceId = workshop.deviceId;
    if (!deviceId) return false;
    return trusted.some(
      (entry) =>
        entry.workshopDeviceId === deviceId ||
        entry.workshopDeviceId.startsWith(deviceId.slice(0, 8)),
    );
  }

  function monogram(label: string): string {
    const parts = label.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return "?";
    if (parts.length === 1) return parts[0]!.slice(0, 1).toUpperCase();
    return `${parts[0]!.slice(0, 1)}${parts[1]!.slice(0, 1)}`.toUpperCase();
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

  async function copyInviteLink() {
    if (!qr?.url) return;
    try {
      await navigator.clipboard.writeText(qr.url);
      copyFlash = true;
      setTimeout(() => {
        copyFlash = false;
      }, 1500);
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
      if (!selectedPeerId && trusted.length > 0) {
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
    await Promise.all([
      refreshInvite(),
      refreshNearby(),
      refreshTrusted(),
      refreshInbox(),
      refreshLanPairing(),
    ]);
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
      // Same path as `medousa peer connect <url>`: fetch /qr over LAN when no invite is pasted.
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

  async function openMessage(message: PeerMessage) {
    // Outbound copies share readAt with the recipient — only they should mark them read.
    if (isOutbound(message) || message.readAt) return;
    try {
      await peerMarkRead(message.id);
      await refreshInbox();
    } catch {
      /* ignore */
    }
  }

  function formatTime(iso: string): string {
    try {
      const date = new Date(iso);
      const now = new Date();
      const sameDay = date.toDateString() === now.toDateString();
      return sameDay
        ? date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
        : date.toLocaleString([], {
            month: "short",
            day: "numeric",
            hour: "numeric",
            minute: "2-digit",
          });
    } catch {
      return iso;
    }
  }

  function openAddPeer() {
    error = null;
    success = null;
    addPeerOpen = true;
    void refreshInvite();
    void refreshNearby();
  }

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
</script>

<section class="peers-panel {visible ? '' : 'hidden'}" aria-label="Peers">
  <aside class="peers-sidebar">
    <header class="peers-sidebar-head">
      <div>
        <h1 class="peers-app-title">Peers</h1>
        <p class="peers-app-sub">People on your network</p>
      </div>
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

    {#if !hasPeers}
      <div class="peers-empty-people">
        <Users size={22} strokeWidth={1.5} />
        <p>No peers yet</p>
        <button type="button" class="btn btn-sm btn-primary" onclick={openAddPeer}>
          Add peer
        </button>
      </div>
    {:else}
      <ul class="peers-people">
        {#each trusted as peer (peer.workshopId)}
          {@const peerUnread = unreadForPeer(peer)}
          <li>
            <button
              type="button"
              class="peers-person"
              class:peers-person-active={selectedPeerId === peer.workshopId}
              onclick={() => {
                selectedPeerId = peer.workshopId;
                peerMenuOpen = false;
                cancelRename();
              }}
            >
              <span class="peers-avatar" aria-hidden="true">{monogram(peer.label)}</span>
              <span class="peers-person-copy">
                <span class="peers-person-name">{peer.label}</span>
                <span class="peers-person-meta">
                  {#if peer.inbound}
                    Connected to you
                  {:else if peer.hasSessionToken}
                    Ready
                  {:else}
                    Reconnect needed
                  {/if}
                </span>
              </span>
              {#if peerUnread > 0}
                <span class="peers-person-unread">{peerUnread}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <div class="peers-main">
    {#if !selectedPeer}
      <div class="peers-empty-main">
        <div class="peers-empty-icon" aria-hidden="true">
          <Users size={28} strokeWidth={1.5} />
        </div>
        <h2>{hasPeers ? "Choose someone" : "Add someone nearby"}</h2>
        <p>
          {#if hasPeers}
            Pick a peer to message, or add another with the + button.
          {:else}
            Show your invite or connect to someone on the same Wi‑Fi.
          {/if}
        </p>
        {#if !hasPeers}
          <button type="button" class="btn btn-sm btn-primary" onclick={openAddPeer}>
            Add peer
          </button>
        {/if}
        {#if unread > 0}
          <p class="peers-unread-banner">{unread} unread</p>
        {/if}
      </div>
    {:else}
      <header class="peers-thread-head">
        <div class="peers-thread-identity">
          <span class="peers-avatar peers-avatar-lg" aria-hidden="true">
            {monogram(renaming ? renameDraft || selectedPeer.label : selectedPeer.label)}
          </span>
          <div>
            {#if renaming}
              <input
                class="peers-rename-input"
                type="text"
                bind:value={renameDraft}
                disabled={busy}
                aria-label="Peer display name"
                autofocus
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void commitRename();
                  } else if (event.key === "Escape") {
                    event.preventDefault();
                    cancelRename();
                  }
                }}
                onblur={() => void commitRename()}
              />
            {:else}
              <h2 class="peers-thread-name">{selectedPeer.label}</h2>
            {/if}
            <p class="peers-thread-status">
              {#if selectedPeer.inbound}
                Connected to you — replies stay here for them to read
              {:else if selectedPeer.hasSessionToken}
                Connected
              {:else}
                Needs reconnect
              {/if}
            </p>
          </div>
        </div>
        <div class="peers-thread-menu-wrap">
          <button
            type="button"
            class="peers-icon-btn"
            aria-label="Peer options"
            aria-expanded={peerMenuOpen}
            onclick={() => (peerMenuOpen = !peerMenuOpen)}
          >
            <MoreHorizontal size={18} />
          </button>
          {#if peerMenuOpen}
            <div class="peers-menu" role="menu">
              {#if !selectedPeer.inbound}
                <button
                  type="button"
                  role="menuitem"
                  class="peers-menu-item"
                  disabled={busy}
                  onclick={startRename}
                >
                  Rename
                </button>
              {/if}
              <button
                type="button"
                role="menuitem"
                class="peers-menu-item peers-menu-danger"
                disabled={busy}
                onclick={() => void revokePeer(selectedPeer.workshopId)}
              >
                Remove peer
              </button>
            </div>
          {/if}
        </div>
      </header>

      <div class="peers-thread">
        {#if threadMessages.length === 0}
          <div class="peers-thread-empty">
            <p>Say hi to {selectedPeer.label.split(/\s+/)[0]}.</p>
          </div>
        {:else}
          {#each [...threadMessages].reverse() as message (message.id)}
            <button
              type="button"
              class="peers-bubble"
              class:peers-bubble-out={isOutbound(message)}
              class:peers-bubble-unread={!isOutbound(message) && !message.readAt}
              onclick={() => void openMessage(message)}
            >
              <p class="peers-bubble-body">{message.body}</p>
              {#if message.attachmentResult}
                <p class="peers-bubble-attach">
                  {message.attachmentResult.summary ??
                    (message.attachmentResult.imported ? "Attachment imported" : "Attachment")}
                </p>
              {/if}
              <span class="peers-bubble-time">
                {isOutbound(message) ? "You · " : ""}{formatTime(message.sentAt)}
              </span>
            </button>
          {/each}
        {/if}
      </div>

      <div class="peers-compose">
        {#if attachmentLabel}
          <div class="peers-attach-chip">
            <Paperclip size={12} />
            <span>{attachmentLabel}</span>
            <button type="button" class="peers-attach-clear" aria-label="Remove attachment" onclick={clearAttachment}>
              <X size={12} />
            </button>
          </div>
        {/if}
        <div class="peers-compose-bar">
          <div class="peers-attach-wrap">
            <button
              type="button"
              class="peers-icon-btn"
              aria-label="Attach"
              aria-expanded={attachMenuOpen}
              disabled={busy || !selectedPeer.hasSessionToken}
              onclick={() => (attachMenuOpen = !attachMenuOpen)}
            >
              <Paperclip size={18} />
            </button>
            {#if attachMenuOpen}
              <div class="peers-menu peers-menu-up" role="menu">
                <button
                  type="button"
                  role="menuitem"
                  class="peers-menu-item"
                  onclick={() => {
                    composeAttachKind = "note";
                    composeNotePath = noteOptions[0]?.path ?? "";
                    attachMenuOpen = false;
                  }}
                >
                  Vault note
                </button>
                <button
                  type="button"
                  role="menuitem"
                  class="peers-menu-item"
                  onclick={() => {
                    composeAttachKind = "artifact";
                    composeArtifactId = artifactOptions[0]?.artifact_id ?? "";
                    attachMenuOpen = false;
                  }}
                >
                  Artifact
                </button>
              </div>
            {/if}
          </div>
          {#if composeAttachKind === "note"}
            <select class="peers-attach-select" bind:value={composeNotePath} disabled={busy}>
              {#each noteOptions as note (note.path)}
                <option value={note.path}>{note.path}</option>
              {/each}
            </select>
          {:else if composeAttachKind === "artifact"}
            <select class="peers-attach-select" bind:value={composeArtifactId} disabled={busy}>
              {#each artifactOptions as item (item.artifact_id)}
                <option value={item.artifact_id}>{item.label ?? item.artifact_id}</option>
              {/each}
            </select>
          {/if}
          <input
            class="peers-compose-input"
            type="text"
            bind:value={composeBody}
            disabled={busy || !selectedPeer.hasSessionToken}
            placeholder="Message {selectedPeer.label.split(/\s+/)[0]}…"
            onkeydown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void sendMessage();
              }
            }}
          />
          <button
            type="button"
            class="peers-send-btn"
            disabled={busy || !selectedPeer.hasSessionToken}
            aria-label="Send"
            onclick={() => void sendMessage()}
          >
            <Send size={16} />
          </button>
        </div>
      </div>
    {/if}

    {#if error}
      <p class="peers-error">{error}</p>
    {/if}
    {#if success}
      <p class="peers-success">{success}</p>
    {/if}
  </div>
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
        <div class="peers-sheet-actions">
          <button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={() => void copyInviteLink()}>
            <Copy size={14} />
            {copyFlash ? "Copied" : "Copy link"}
          </button>
          <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={() => void rotateInvite()}>
            <RefreshCw size={14} />
            Refresh
          </button>
        </div>
      {:else}
        <p class="peers-sheet-lead">Loading invite…</p>
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
          type="text"
          bind:value={fallbackDaemonUrl}
          placeholder="http://10.12.0.13:7419"
          disabled={busy}
          autofocus
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

<style>
  .peers-panel {
    display: flex;
    height: 100%;
    min-height: 0;
    background: rgb(var(--color-surface-950));
  }

  .peers-sidebar {
    width: min(17.5rem, 34%);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem 0.75rem;
    border-right: 1px solid color-mix(in srgb, var(--color-surface-700) 40%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 55%, transparent);
  }

  .peers-sidebar-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0 0.35rem;
  }

  .peers-app-title {
    margin: 0;
    font-size: 1.125rem;
    font-weight: 650;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .peers-app-sub {
    margin: 0.15rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-500));
  }

  .peers-add-btn,
  .peers-icon-btn,
  .peers-send-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .peers-add-btn {
    width: 2.1rem;
    height: 2.1rem;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 28%, transparent);
  }

  .peers-icon-btn {
    width: 2rem;
    height: 2rem;
    background: transparent;
  }

  .peers-icon-btn:hover,
  .peers-add-btn:hover {
    background: color-mix(in srgb, var(--color-primary-500) 22%, transparent);
  }

  .peers-nearby-banner {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.55rem 0.6rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
    color: rgb(var(--color-primary-200));
  }

  .peers-nearby-banner-copy {
    flex: 1 1 auto;
    min-width: 0;
  }

  .peers-nearby-banner-title {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .peers-nearby-banner-meta {
    margin: 0.1rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-400));
  }

  .peers-nearby-more {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .peers-nearby-chip {
    border: 0;
    border-radius: 999px;
    padding: 0.25rem 0.55rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-700) 50%, transparent);
    cursor: pointer;
  }

  .peers-people {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
    overflow: auto;
  }

  .peers-person {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    width: 100%;
    border: 0;
    border-radius: 0.75rem;
    padding: 0.55rem 0.5rem;
    text-align: left;
    color: inherit;
    background: transparent;
    cursor: pointer;
  }

  .peers-person:hover {
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .peers-person-active {
    background: color-mix(in srgb, var(--color-primary-500) 14%, transparent);
  }

  .peers-avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 700;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 28%, transparent);
  }

  .peers-avatar-lg {
    width: 2.5rem;
    height: 2.5rem;
    font-size: 0.8125rem;
  }

  .peers-person-copy {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .peers-person-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .peers-person-meta {
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-500));
  }

  .peers-person-unread {
    display: inline-flex;
    min-width: 1.2rem;
    justify-content: center;
    border-radius: 999px;
    padding: 0.1rem 0.35rem;
    font-size: 0.625rem;
    font-weight: 700;
    color: rgb(var(--color-primary-50));
    background: rgb(var(--color-primary-500));
  }

  .peers-empty-people {
    display: grid;
    justify-items: center;
    gap: 0.45rem;
    margin: auto 0;
    padding: 1rem 0.5rem;
    text-align: center;
    color: rgb(var(--color-surface-500));
    font-size: 0.8125rem;
  }

  .peers-main {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .peers-empty-main {
    margin: auto;
    max-width: 20rem;
    text-align: center;
    color: rgb(var(--color-surface-400));
  }

  .peers-empty-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
  }

  .peers-empty-main h2 {
    margin: 0.85rem 0 0.35rem;
    font-size: 1.125rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .peers-empty-main p {
    margin: 0 0 0.85rem;
    font-size: 0.8125rem;
    line-height: 1.5;
  }

  .peers-unread-banner {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-primary-200));
  }

  .peers-thread-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.85rem 1.15rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 40%, transparent);
  }

  .peers-thread-identity {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    min-width: 0;
  }

  .peers-thread-name {
    margin: 0;
    font-size: 1rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .peers-rename-input {
    display: block;
    width: min(16rem, 100%);
    margin: 0;
    border: 1px solid color-mix(in srgb, var(--color-primary-400) 55%, transparent);
    border-radius: 0.4rem;
    padding: 0.2rem 0.45rem;
    font-size: 1rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-surface-900) 80%, transparent);
    outline: none;
  }

  .peers-thread-status {
    margin: 0.1rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-500));
  }

  .peers-thread-menu-wrap,
  .peers-attach-wrap {
    position: relative;
  }

  .peers-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 0.25rem);
    z-index: 20;
    min-width: 9rem;
    padding: 0.3rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.35);
  }

  .peers-menu-up {
    top: auto;
    bottom: calc(100% + 0.25rem);
    left: 0;
    right: auto;
  }

  .peers-menu-item {
    display: block;
    width: 100%;
    border: 0;
    border-radius: 0.4rem;
    padding: 0.4rem 0.55rem;
    text-align: left;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-200));
    background: transparent;
    cursor: pointer;
  }

  .peers-menu-item:hover {
    background: color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .peers-menu-danger {
    color: rgb(var(--color-error-300));
  }

  .peers-thread {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    padding: 1rem 1.15rem;
  }

  .peers-thread-empty {
    margin: auto;
    text-align: center;
    color: rgb(var(--color-surface-500));
    font-size: 0.875rem;
  }

  .peers-bubble {
    align-self: flex-start;
    max-width: min(28rem, 88%);
    border: 0;
    border-radius: 1rem 1rem 1rem 0.35rem;
    padding: 0.65rem 0.8rem 0.45rem;
    text-align: left;
    color: inherit;
    background: color-mix(in srgb, var(--color-surface-800) 70%, transparent);
    cursor: pointer;
  }

  .peers-bubble-out {
    align-self: flex-end;
    border-radius: 1rem 1rem 0.35rem 1rem;
    background: color-mix(in srgb, var(--color-primary-700) 45%, var(--color-surface-900));
  }

  .peers-bubble-unread {
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-primary-500) 40%, transparent);
  }

  .peers-bubble-body {
    margin: 0;
    font-size: 0.875rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-100));
    white-space: pre-wrap;
  }

  .peers-bubble-attach {
    margin: 0.35rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-primary-250, var(--color-primary-200)));
  }

  .peers-bubble-time {
    display: block;
    margin-top: 0.3rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-500));
  }

  .peers-compose {
    padding: 0.65rem 1rem 1rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 40%, transparent);
  }

  .peers-attach-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-bottom: 0.45rem;
    border-radius: 999px;
    padding: 0.2rem 0.45rem 0.2rem 0.55rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-primary-100));
    background: color-mix(in srgb, var(--color-primary-500) 18%, transparent);
  }

  .peers-attach-clear {
    display: inline-flex;
    border: 0;
    border-radius: 999px;
    padding: 0.1rem;
    color: inherit;
    background: transparent;
    cursor: pointer;
  }

  .peers-compose-bar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 70%, transparent);
    padding: 0.3rem 0.35rem 0.3rem 0.35rem;
  }

  .peers-compose-input,
  .peers-attach-select,
  .peers-field input {
    border: 0;
    background: transparent;
    color: rgb(var(--color-surface-100));
    font: inherit;
  }

  .peers-compose-input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.4rem 0.35rem;
    outline: none;
  }

  .peers-attach-select {
    max-width: 8rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
  }

  .peers-send-btn {
    width: 2.15rem;
    height: 2.15rem;
    color: rgb(var(--color-primary-50));
    background: color-mix(in srgb, var(--color-primary-500) 55%, transparent);
  }

  .peers-send-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .peers-error,
  .peers-success {
    margin: 0 1rem 0.75rem;
    font-size: 0.75rem;
  }

  .peers-error {
    color: rgb(var(--color-error-300));
  }

  .peers-success {
    color: rgb(var(--color-success-300));
  }

  .peers-sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(0 0 0 / 0.55);
    padding: 1rem;
  }

  .peers-sheet {
    width: min(22rem, 100%);
    border-radius: 1rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 50%, transparent);
    background: rgb(var(--color-surface-900));
    padding: 1rem 1.1rem 1.15rem;
    box-shadow: 0 18px 48px rgb(0 0 0 / 0.45);
  }

  .peers-sheet-sm {
    width: min(20rem, 100%);
  }

  .peers-sheet-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .peers-sheet-head h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 650;
  }

  .peers-sheet-lead {
    margin: 0.45rem 0 0.75rem;
    font-size: 0.8125rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .peers-visible-pill {
    display: inline-flex;
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    font-size: 0.625rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: rgb(var(--color-success-200));
    background: color-mix(in srgb, var(--color-success-500) 16%, transparent);
  }

  .peers-qr {
    display: block;
    width: min(11.5rem, 100%);
    margin: 0.85rem auto 0.4rem;
    border-radius: 0.75rem;
    background: white;
    padding: 0.55rem;
  }

  .peers-code {
    margin: 0;
    text-align: center;
    font-size: 1rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: rgb(var(--color-surface-100));
  }

  .peers-countdown {
    margin: 0.25rem 0 0;
    text-align: center;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-500));
  }

  .peers-sheet-actions {
    display: flex;
    justify-content: center;
    gap: 0.45rem;
    margin-top: 0.85rem;
  }

  .peers-sheet-nearby {
    margin-top: 1rem;
    padding-top: 0.85rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .peers-sheet-nearby h3 {
    margin: 0 0 0.45rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
  }

  .peers-sheet-nearby-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.35rem;
  }

  .peers-sheet-nearby-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }

  .peers-paste-link {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.85rem;
    border: 0;
    padding: 0;
    font-size: 0.75rem;
    color: rgb(var(--color-primary-300));
    background: transparent;
    cursor: pointer;
  }

  .peers-field {
    display: grid;
    gap: 0.25rem;
    margin-top: 0.65rem;
    font-size: 0.75rem;
  }

  .peers-field span {
    color: rgb(var(--color-surface-400));
  }

  .peers-field-optional {
    color: rgb(var(--color-surface-500));
    font-weight: 400;
  }

  .peers-sheet-hint {
    margin: 0.35rem 0 0;
    font-size: 0.75rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }

  .peers-sheet-hint code {
    font-size: 0.7rem;
    color: rgb(var(--color-surface-300));
  }

  .peers-lan-toggle {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin: 0.65rem 0 0.35rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-200));
    cursor: pointer;
  }

  .peers-field input {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 50%, transparent);
    padding: 0.4rem 0.5rem;
  }

  @media (max-width: 860px) {
    .peers-panel {
      flex-direction: column;
    }

    .peers-sidebar {
      width: 100%;
      max-height: 40%;
      border-right: 0;
      border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 40%, transparent);
    }
  }
</style>
