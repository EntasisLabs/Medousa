<script lang="ts">
  import type { GraphemeRecipe } from "$lib/grapheme/graphemeRecipes";
  import { GRAPHEME_STARTER_RECIPES } from "$lib/grapheme/graphemeRecipes";

  interface Props {
    title?: string;
    hint?: string;
    recipes?: GraphemeRecipe[];
    compact?: boolean;
    onselect: (recipe: GraphemeRecipe) => void;
  }

  let {
    title = "Start with a recipe",
    hint = "Pick one — run it, tweak it, save it, turn it into a flow.",
    recipes = GRAPHEME_STARTER_RECIPES,
    compact = false,
    onselect,
  }: Props = $props();
</script>

<div class="workshop-recipe-grid {compact ? 'workshop-recipe-grid-compact' : ''}">
  <div class="mb-3">
    <p class="text-sm font-medium text-surface-100">{title}</p>
    {#if hint}
      <p class="workshop-faint mt-1 text-xs leading-relaxed">{hint}</p>
    {/if}
  </div>
  <div class="grid gap-2 {compact ? 'sm:grid-cols-1' : 'sm:grid-cols-3'}">
    {#each recipes as recipe (recipe.id)}
      <button
        type="button"
        class="workshop-recipe-card text-left"
        onclick={() => onselect(recipe)}
      >
        <p class="text-sm font-medium text-surface-50">{recipe.title}</p>
        <p class="workshop-faint mt-1 text-[11px] leading-relaxed">{recipe.subtitle}</p>
      </button>
    {/each}
  </div>
</div>
