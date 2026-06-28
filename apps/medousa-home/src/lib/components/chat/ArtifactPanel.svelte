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
    onExpand?: () => void;
  }

  let { open, sessionId, artifact, onClose, onExpand }: Props = $props();

  function handleClose() {
    haptic("light");
    onClose();
  }

  function handleExpand() {
    haptic("light");
    onExpand?.();
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
      class="artifact-chrome-backdrop artifact-panel-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) handleClose();
      }}
    >
      <aside
        class="artifact-chrome-stage artifact-panel-stage"
        aria-label={artifact.label}
      >
        <ArtifactPresentationChrome
          title={artifact.label}
          onClose={handleClose}
          onExpand={onExpand ? handleExpand : undefined}
          leadingClose={layout.isMobile}
          sessionId={sessionId}
          artifactId={artifact.artifactId}
        >
          <ArtifactEmbed
            {sessionId}
            artifactId={artifact.artifactId}
            label={artifact.label}
            mime={artifact.mime}
            heightPx={artifact.heightPx ?? 720}
            bare={true}
            mode="panel"
          />
        </ArtifactPresentationChrome>
      </aside>
    </div>
  </BodyPortal>
{/if}

<style>
  .artifact-panel-backdrop {
    z-index: 100;
    display: flex;
    justify-content: flex-end;
    animation: artifact-backdrop-in 160ms ease-out;
  }

  .artifact-panel-stage {
    width: min(44rem, 100vw);
    height: 100%;
    border-left: 1px solid color-mix(in srgb, var(--color-surface-600) 45%, transparent);
    box-shadow: -20px 0 56px rgb(0 0 0 / 0.22);
    animation: artifact-panel-in 220ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  :global(html:not(.dark)) .artifact-panel-stage {
    box-shadow: -16px 0 40px rgb(0 0 0 / 0.08);
  }

  @keyframes artifact-backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes artifact-panel-in {
    from {
      transform: translateX(100%);
    }
    to {
      transform: translateX(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .artifact-panel-backdrop,
    .artifact-panel-stage {
      animation: none;
    }
  }
</style>
