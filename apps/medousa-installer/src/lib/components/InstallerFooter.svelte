<script lang="ts">
  import { truncatePath } from "../copy";

  interface Props {
    installRoot: string;
    sizeLabel: string;
    primaryLabel?: string;
    busy?: boolean;
    disabled?: boolean;
    showBack?: boolean;
    onPrimary?: () => void;
    onPickLocation?: () => void;
    onBack?: () => void;
    onLicense?: () => void;
  }

  let {
    installRoot,
    sizeLabel,
    primaryLabel = "Install",
    busy = false,
    disabled = false,
    showBack = false,
    onPrimary,
    onPickLocation,
    onBack,
    onLicense,
  }: Props = $props();
</script>

<div class="installer-footer">
  <div class="footer-left">
    {#if showBack}
      <button type="button" class="btn-secondary" onclick={onBack}>Back</button>
    {/if}
    <div class="location">
      <span class="path" title={installRoot}>{truncatePath(installRoot)}</span>
      <button type="button" class="link" onclick={onPickLocation}>Change…</button>
    </div>
  </div>

  <div class="footer-right">
    <div class="total">
      <span class="total-label">Total space required</span>
      <span class="total-value">{sizeLabel}</span>
    </div>
    <button
      type="button"
      class="btn-primary"
      disabled={disabled || busy}
      onclick={onPrimary}
    >
      {primaryLabel}
    </button>
  </div>
</div>

{#if onLicense}
  <div class="legal">
    <button type="button" class="link legal-link" onclick={onLicense}>License terms</button>
  </div>
{/if}

<style>
  .installer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.85rem 1rem;
    min-height: var(--installer-footer-height);
  }

  .footer-left {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    min-width: 0;
    flex: 1;
  }

  .location {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    font-size: var(--installer-caption-size);
    color: var(--installer-muted);
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .footer-right {
    display: flex;
    align-items: center;
    gap: 1.25rem;
    flex-shrink: 0;
  }

  .total {
    text-align: right;
  }

  .total-label {
    display: block;
    font-size: 0.6875rem;
    color: var(--installer-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .total-value {
    font-size: 1rem;
    font-weight: 600;
    color: var(--installer-text);
  }

  .btn-primary {
    background: var(--installer-accent);
    color: white;
    border: none;
    border-radius: var(--installer-radius-control);
    padding: 0.6rem 1.35rem;
    font-weight: 600;
    font-size: var(--installer-body-size);
    transition: background var(--installer-motion);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--installer-accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .btn-primary:focus-visible,
  .btn-secondary:focus-visible,
  .link:focus-visible {
    outline: 2px solid var(--installer-accent);
    outline-offset: 2px;
  }

  .btn-secondary {
    background: transparent;
    border: 1px solid var(--installer-border-strong);
    color: var(--installer-text-secondary);
    border-radius: var(--installer-radius-control);
    padding: 0.5rem 0.85rem;
    font-size: var(--installer-caption-size);
  }

  .link {
    background: none;
    border: none;
    color: var(--installer-accent);
    padding: 0;
    font-size: inherit;
    cursor: pointer;
  }

  .legal {
    padding: 0 1rem 0.5rem;
    text-align: right;
  }

  .legal-link {
    font-size: 0.6875rem;
    color: var(--installer-faint);
  }
</style>
