<script lang="ts">
  import { ExternalLink, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { attachmentFileName } from "$lib/utils/vaultAttachments";
  import VaultAttachmentPreviewContent from "./VaultAttachmentPreviewContent.svelte";

  const open = $derived(
    vault.previewPresentation === "panel" && vault.previewingAttachment != null,
  );
  const attachment = $derived(vault.previewingAttachment);

  function handleClose() {
    haptic("light");
    vault.closeAttachmentPreview();
  }

  async function handleOpenExternal() {
    if (!attachment) return;
    haptic("light");
    await openAttachmentPath(attachment.path);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    handleClose();
  }

  $effect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      handleClose();
      return true;
    });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && attachment}
  <BodyPortal>
    <div
      class="artifact-chrome-backdrop artifact-fullscreen-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={attachment.label || attachmentFileName(attachment)}
      tabindex="-1"
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
      onkeydown={handleKeydown}
    >
      <div class="artifact-chrome-stage artifact-fullscreen-stage">
        <div class="artifact-chrome">
          <header
            class="artifact-chrome-bar"
            class:artifact-chrome-bar-leading-close={layout.isMobile}
          >
            {#if layout.isMobile}
              <button
                type="button"
                class="artifact-chrome-btn artifact-chrome-btn-close-leading"
                aria-label="Close"
                onclick={handleClose}
              >
                <X size={16} strokeWidth={2} aria-hidden="true" />
                <span>Close</span>
              </button>
            {/if}
            <div class="artifact-chrome-title-wrap">
              <h3 class="artifact-chrome-title">
                {attachment.label || attachmentFileName(attachment)}
              </h3>
              <p class="truncate text-[11px] text-surface-500">{attachment.path}</p>
            </div>
            <div class="artifact-chrome-actions">
              <button
                type="button"
                class="artifact-chrome-btn artifact-chrome-btn-secondary"
                aria-label="Open in app"
                onclick={() => void handleOpenExternal()}
              >
                <ExternalLink size={14} strokeWidth={2} aria-hidden="true" />
                <span>Open in app</span>
              </button>
              {#if !layout.isMobile}
                <button
                  type="button"
                  class="artifact-chrome-btn"
                  aria-label="Close"
                  onclick={handleClose}
                >
                  <X size={14} strokeWidth={2} aria-hidden="true" />
                  <span>Close</span>
                </button>
              {/if}
            </div>
          </header>
          <div class="artifact-chrome-body">
            <VaultAttachmentPreviewContent {attachment} fill={true} />
          </div>
        </div>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .artifact-fullscreen-backdrop {
    z-index: 110;
    display: flex;
    align-items: stretch;
    justify-content: center;
    padding: max(0.75rem, env(safe-area-inset-top, 0px))
      max(0.75rem, env(safe-area-inset-right, 0px))
      max(0.75rem, env(safe-area-inset-bottom, 0px))
      max(0.75rem, env(safe-area-inset-left, 0px));
    animation: artifact-backdrop-in 180ms ease-out;
  }

  .artifact-fullscreen-stage {
    display: flex;
    flex-direction: column;
    width: min(72rem, calc(100vw - 1.5rem));
    height: 100%;
    min-height: 0;
    border-radius: 1.125rem;
    animation: artifact-stage-in 240ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  @keyframes artifact-backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes artifact-stage-in {
    from {
      opacity: 0;
      transform: scale(0.98) translateY(6px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  @media (max-width: 640px) {
    .artifact-fullscreen-stage {
      width: 100%;
      border-radius: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .artifact-fullscreen-backdrop,
    .artifact-fullscreen-stage {
      animation: none;
    }
  }
</style>
