<script lang="ts">
  import { calendar } from "$lib/stores/calendar.svelte";
</script>

<button type="button" class="mobile-calendar-today" onclick={() => calendar.goToday()}>
  Today
</button>

<style>
  .mobile-calendar-today {
    position: absolute;
    left: 1rem;
    bottom: calc(0.9rem + env(safe-area-inset-bottom, 0px));
    z-index: 12;
    min-height: 2.75rem;
    border: 1px solid rgb(var(--shell-border) / 0.78);
    border-radius: 9999px;
    background: color-mix(in srgb, rgb(var(--shell-pane-bg)) 88%, transparent);
    padding: 0.35rem 1rem;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--theme-text));
    box-shadow: 0 8px 24px rgb(var(--theme-shadow) / 0.18);
    backdrop-filter: blur(18px) saturate(1.2);
  }

  .mobile-calendar-today:active {
    transform: scale(0.97);
    background: rgb(var(--color-surface-800) / 0.9);
  }
</style>
