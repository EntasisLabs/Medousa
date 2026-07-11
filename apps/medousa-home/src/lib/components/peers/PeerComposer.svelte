<script lang="ts">
  import { Paperclip, Send, X } from "@lucide/svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";

  interface NoteOption {
    path: string;
  }

  interface ArtifactOption {
    artifact_id: string;
    label?: string | null;
  }

  interface Props {
    peerLabel: string;
    body?: string;
    busy?: boolean;
    canSend?: boolean;
    needsReconnect?: boolean;
    attachKind?: "none" | "note" | "artifact";
    notePath?: string;
    artifactId?: string;
    noteOptions?: NoteOption[];
    artifactOptions?: ArtifactOption[];
    attachMenuOpen?: boolean;
    onToggleAttachMenu: () => void;
    onPickNote: (path: string) => void;
    onPickArtifact: (id: string) => void;
    onClearAttachment: () => void;
    onSend: () => void;
    onReconnect: () => void;
  }

  let {
    peerLabel,
    body = $bindable(""),
    busy = false,
    canSend = true,
    needsReconnect = false,
    attachKind = "none",
    notePath = "",
    artifactId = "",
    noteOptions = [],
    artifactOptions = [],
    attachMenuOpen = false,
    onToggleAttachMenu,
    onPickNote,
    onPickArtifact,
    onClearAttachment,
    onSend,
    onReconnect,
  }: Props = $props();

  const attachmentLabel = $derived.by(() => {
    if (attachKind === "note" && notePath) return notePath;
    if (attachKind === "artifact" && artifactId) {
      return (
        artifactOptions.find((item) => item.artifact_id === artifactId)?.label ?? artifactId
      );
    }
    return null;
  });

  const firstName = $derived(peerLabel.split(/\s+/)[0] || "peer");
</script>

<div class="peers-compose">
  {#if needsReconnect}
    <div class="peers-compose-reconnect">
      <p>Connection expired — reconnect to keep messaging.</p>
      <button type="button" class="peers-reconnect-btn" disabled={busy} onclick={onReconnect}>
        Reconnect
      </button>
    </div>
  {/if}

  {#if attachmentLabel}
    <div class="peers-attach-chip">
      <Paperclip size={12} />
      <span>{attachmentLabel}</span>
      <button
        type="button"
        class="peers-attach-clear"
        aria-label="Remove attachment"
        onclick={onClearAttachment}
      >
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
        disabled={busy || !canSend}
        onclick={onToggleAttachMenu}
      >
        <Paperclip size={18} />
      </button>
      {#if attachMenuOpen}
        <div class="peers-attach-picker" role="menu">
          <p class="peers-attach-picker-label">Vault notes</p>
          {#if noteOptions.length === 0}
            <p class="peers-attach-picker-empty">No notes yet</p>
          {:else}
            {#each noteOptions as note (note.path)}
              <button
                type="button"
                role="menuitem"
                class="peers-attach-picker-item"
                onclick={() => onPickNote(note.path)}
              >
                {note.path}
              </button>
            {/each}
          {/if}
          <p class="peers-attach-picker-label">Artifacts</p>
          {#if artifactOptions.length === 0}
            <p class="peers-attach-picker-empty">No artifacts yet</p>
          {:else}
            {#each artifactOptions as item (item.artifact_id)}
              <button
                type="button"
                role="menuitem"
                class="peers-attach-picker-item"
                onclick={() => onPickArtifact(item.artifact_id)}
              >
                {item.label ?? item.artifact_id}
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <GrowingTextarea
      class="peers-compose-input"
      bind:value={body}
      disabled={busy || !canSend}
      placeholder="Message {firstName}…"
      aria-label="Message {firstName}"
      minHeight={36}
      maxHeight={128}
      onkeydown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          if (canSend && !busy) onSend();
        }
      }}
    />

    <button
      type="button"
      class="peers-send-btn"
      disabled={busy || !canSend}
      aria-label="Send"
      onclick={onSend}
    >
      <Send size={16} />
    </button>
  </div>
</div>
