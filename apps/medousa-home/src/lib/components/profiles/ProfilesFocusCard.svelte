<script lang="ts">
  import type { IdentityFieldBlob } from "$lib/types/identityField";
  import { MessageCircle, X } from "@lucide/svelte";

  interface Props {
    blob: IdentityFieldBlob | null;
    portrait: string;
    onClose: () => void;
    onOpenChat?: () => void;
    onCorrect?: () => void;
  }

  let { blob, portrait, onClose, onOpenChat, onCorrect }: Props = $props();

  const personRole = $derived.by(() => {
    if (!blob || blob.kind !== "person") return "";
    return blob.subtitle?.trim() || "someone in your life";
  });

  const voiceLine = $derived.by(() => {
    if (!blob) return "";
    if (blob.kind === "cluster" && !blob.entry) {
      return portrait;
    }
    if (blob.kind === "person") {
      return `${blob.label} — ${personRole}. Someone she should recognize in every thread.`;
    }
    if (blob.kind === "preference") {
      return `She shows up for you with ${blob.label.toLowerCase()} in mind — ${blob.subtitle}.`;
    }
    if (blob.entry?.kind === "claim") {
      return blob.subtitle !== blob.label ? blob.subtitle : blob.label;
    }
    return blob.subtitle || blob.label;
  });
</script>

{#if blob}
  <div class="profiles-focus-card" role="dialog" aria-label={blob.label}>
    <div class="profiles-focus-card-inner">
      <button
        type="button"
        class="profiles-focus-close mobile-icon-btn"
        aria-label="Close"
        onclick={() => onClose()}
      >
        <X size={16} strokeWidth={1.75} />
      </button>
      <p class="profiles-focus-kicker">
        {#if blob.kind === "cluster" && !blob.entry}
          Who she knows you as
        {:else if blob.kind === "person"}
          In your circle
        {:else if blob.kind === "preference"}
          Rhythm
        {:else}
          She remembers
        {/if}
      </p>
      <h2 class="profiles-focus-title">{blob.label}</h2>
      <p class="profiles-focus-voice">{voiceLine}</p>
      <div class="profiles-focus-actions">
        {#if onOpenChat}
          <button type="button" class="btn btn-sm variant-soft-primary" onclick={() => onOpenChat?.()}>
            <MessageCircle size={14} class="mr-1" aria-hidden="true" />
            Talk in chat
          </button>
        {/if}
        {#if onCorrect}
          <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => onCorrect?.()}>
            That's not right
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
