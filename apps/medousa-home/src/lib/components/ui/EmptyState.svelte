<script lang="ts">
  interface Props {
    title: string;
    description: string;
    children?: import("svelte").Snippet;
  }

  let { title, description, children }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 text-center">
  <div class="mb-4 h-0.5 w-10 rounded-full bg-primary-500"></div>
  <p class="text-base font-medium text-surface-100">{title}</p>
  <p class="mt-2 max-w-sm text-sm leading-relaxed text-surface-400">
    {description}
  </p>
  {#if children}
    <div class="mt-4">
      {@render children()}
    </div>
  {/if}
</div>
