<script lang="ts">
  import { fetchArtifact } from "$lib/daemon";
  import { friendlyUserError } from "$lib/utils/normieErrors";

  interface Props {
    sessionId: string;
    artifactId: string;
    label: string;
    mime: string;
    heightPx?: number | null;
    compact?: boolean;
  }

  let {
    sessionId,
    artifactId,
    label,
    mime,
    heightPx = 360,
    compact = false,
  }: Props = $props();

  let html = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let frameHeight = $state(heightPx ?? 360);

  $effect(() => {
    const sid = sessionId;
    const aid = artifactId;
    html = null;
    error = null;
    loading = true;
    frameHeight = heightPx ?? 360;

    void (async () => {
      try {
        const response = await fetchArtifact(sid, aid);
        if (!response.mime.includes("html")) {
          error = "This artifact is not HTML.";
          return;
        }
        html = response.body;
      } catch (err) {
        error = friendlyUserError(err instanceof Error ? err.message : String(err));
      } finally {
        loading = false;
      }
    })();
  });

  function handleFrameLoad(event: Event) {
    const frame = event.currentTarget as HTMLIFrameElement;
    try {
      const doc = frame.contentDocument;
      const body = doc?.body;
      if (!body) return;
      const next = Math.min(Math.max(body.scrollHeight + 16, 120), heightPx ?? 360);
      frameHeight = next;
    } catch {
      // sandbox without same-origin — keep configured height
    }
  }
</script>

<div class="artifact-embed {compact ? 'artifact-embed-compact' : ''}">
  {#if loading}
    <p class="artifact-embed-status">Loading {label}…</p>
  {:else if error}
    <p class="artifact-embed-error">{error}</p>
  {:else if html}
    <iframe
      class="artifact-embed-frame"
      title={label}
      sandbox="allow-scripts"
      srcdoc={html}
      style={`height: ${frameHeight}px`}
      onload={handleFrameLoad}
    ></iframe>
  {/if}
</div>

<style>
  .artifact-embed {
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 70%, transparent);
    border-radius: 0.75rem;
    background: color-mix(in srgb, var(--color-surface-900) 88%, transparent);
  }

  .artifact-embed-compact {
    border-radius: 0.625rem;
  }

  .artifact-embed-frame {
    display: block;
    width: 100%;
    border: 0;
    background: #fff;
  }

  .artifact-embed-status,
  .artifact-embed-error {
    margin: 0;
    padding: 0.75rem 1rem;
    font-size: 0.75rem;
    line-height: 1.4;
  }

  .artifact-embed-status {
    color: var(--color-surface-400);
  }

  .artifact-embed-error {
    color: var(--color-warning-300);
  }
</style>
