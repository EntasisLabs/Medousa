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

  const voiceLine = $derived.by(() => {
    if (!blob) return "";
    if (blob.kind === "cluster" && !blob.entry) {
      return portrait;
    }
    const title = blob.entry?.title ?? blob.label;
    return blob.subtitle !== title ? blob.subtitle : "";
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
          About you
        {:else if blob.kind === "person"}
          Person
        {:else if blob.kind === "preference"}
          Preference
        {:else}
          Saved fact
        {/if}
      </p>
      <h2 class="profiles-focus-title">{blob.entry?.title ?? blob.label}</h2>
      {#if voiceLine}<p class="profiles-focus-voice">{voiceLine}</p>{/if}
      <div class="profiles-focus-actions">
        {#if onOpenChat}
          <button type="button" class="btn btn-sm variant-soft-primary" onclick={() => onOpenChat?.()}>
            <MessageCircle size={14} class="mr-1" aria-hidden="true" />
            Talk in chat
          </button>
        {/if}
        {#if onCorrect}
          <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => onCorrect?.()}>
            Edit
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
