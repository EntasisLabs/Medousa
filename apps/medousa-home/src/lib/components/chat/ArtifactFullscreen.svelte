<script lang="ts">
  import ArtifactEmbed from "$lib/components/chat/ArtifactEmbed.svelte";
  import ArtifactPresentationChrome from "$lib/components/chat/ArtifactPresentationChrome.svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { layout } from "$lib/stores/layout.svelte";
  import type { UiArtifact } from "$lib/types/chat";

  interface Props {
    open: boolean;
    sessionId: string;
    artifact: UiArtifact;
    onClose: () => void;
  }

  let { open, sessionId, artifact, onClose }: Props = $props();

  function handleClose() {
    haptic("light");
    onClose();
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

{#if open}
  <BodyPortal>
    <div
      class="artifact-chrome-backdrop artifact-fullscreen-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={artifact.label}
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
    >
      <div class="artifact-chrome-stage artifact-fullscreen-stage">
        <ArtifactPresentationChrome
          title={artifact.label}
          onClose={handleClose}
          leadingClose={layout.isMobile}
          sessionId={sessionId}
          artifactId={artifact.artifactId}
        >
          <ArtifactEmbed
            {sessionId}
            artifactId={artifact.artifactId}
            label={artifact.label}
            mime={artifact.mime}
            heightPx={artifact.heightPx ?? 900}
            bare={true}
            mode="fullscreen"
          />
        </ArtifactPresentationChrome>
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
