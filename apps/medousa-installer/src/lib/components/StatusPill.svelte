<script lang="ts">
  interface Props {
    label: string;
    variant?: "installed" | "update" | "default";
  }

  let { label, variant = "default" }: Props = $props();
</script>

<span class="status-pill {variant}">{label}</span>

<style>
  .status-pill {
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: capitalize;
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    white-space: nowrap;
  }

  .status-pill.installed {
    background: rgb(52 211 153 / 0.15);
    color: var(--installer-success);
  }

  .status-pill.update {
    background: rgb(251 191 36 / 0.15);
    color: var(--installer-warning);
  }

  .status-pill.default {
    background: var(--installer-surface-raised);
    color: var(--installer-muted);
  }
</style>
