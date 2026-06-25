<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    onchange?: () => void;
  }

  let { checked = false, disabled = false, onchange }: Props = $props();
</script>

<button
  type="button"
  class="installer-checkbox"
  class:checked
  class:disabled
  {disabled}
  role="checkbox"
  aria-checked={checked}
  onclick={() => !disabled && onchange?.()}
>
  {#if checked}
    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
      <path
        d="M2 6l3 3 5-5"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  {/if}
</button>

<style>
  .installer-checkbox {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 1.5px solid var(--installer-border-strong);
    background: var(--installer-surface);
    color: white;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    flex-shrink: 0;
    transition:
      border-color var(--installer-motion),
      background var(--installer-motion),
      box-shadow var(--installer-motion);
  }

  .installer-checkbox.checked {
    background: var(--installer-accent);
    border-color: var(--installer-accent);
  }

  .installer-checkbox:focus-visible {
    outline: 2px solid var(--installer-accent);
    outline-offset: 2px;
  }

  .installer-checkbox.disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
