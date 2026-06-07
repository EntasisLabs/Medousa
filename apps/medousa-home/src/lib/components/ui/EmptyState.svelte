<script lang="ts">
  interface Props {
    title: string;
    description: string;
    children?: import("svelte").Snippet;
  }

  let { title, description, children }: Props = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 text-center">
  <p class="text-sm font-medium text-surface-200">{title}</p>
  <p class="workshop-faint mt-1 max-w-sm leading-relaxed">
    {description}
  </p>
  {#if children}
    <div class="mt-3">
      {@render children()}
    </div>
  {/if}
</div>
