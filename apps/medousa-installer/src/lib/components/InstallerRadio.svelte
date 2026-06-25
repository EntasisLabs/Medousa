<script lang="ts">
  interface Props {
    checked?: boolean;
    name?: string;
    disabled?: boolean;
    onchange?: () => void;
    children?: import("svelte").Snippet;
  }

  let { checked = false, name, disabled = false, onchange, children }: Props = $props();
</script>

<label class="installer-radio" class:selected={checked} class:disabled>
  <input
    type="radio"
    {name}
    {checked}
    {disabled}
    class="native"
    onchange={() => onchange?.()}
  />
  {#if children}
    {@render children()}
  {/if}
</label>

<style>
  .installer-radio {
    display: block;
    position: relative;
    cursor: pointer;
  }

  .native {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  .installer-radio:focus-within {
    outline: 2px solid var(--installer-accent);
    outline-offset: 2px;
    border-radius: var(--installer-radius-card);
  }

  .installer-radio.disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
