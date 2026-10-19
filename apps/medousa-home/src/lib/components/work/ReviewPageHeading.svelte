<script lang="ts">
  import { ArrowRight, UserRound } from "@lucide/svelte";
  let { title, author = "", revision = "", branch = "", target = "", updatedAt = "" }: {
    title: string; author?: string; revision?: string; branch?: string; target?: string; updatedAt?: string;
  } = $props();
</script>

<div class="review-reading-heading">
  <h1>{title}</h1>
  <div class="byline">
    {#if author}<span class="author"><UserRound size={12} />{author}</span>{/if}
    {#if updatedAt}<time datetime={updatedAt}>{new Date(updatedAt).toLocaleDateString(undefined, { month: "short", day: "numeric" })}</time>{/if}
    {#if revision}<span>{revision}</span>{/if}
    {#if target}<span class="branches">{#if branch}<code title={branch}>{branch}</code><ArrowRight size={12} />{/if}<code title={target}>{target}</code></span>{/if}
  </div>
</div>

<style>
  .review-reading-heading { min-width:0; margin-bottom:1.75rem; color:rgb(var(--theme-text)); }
  h1 { margin:0; font-size:1.35rem; line-height:1.35; letter-spacing:-.025em; font-weight:550; overflow-wrap:anywhere; }
  .byline { display:flex; flex-wrap:wrap; align-items:center; gap:.4rem .65rem; margin-top:.7rem; color:rgb(var(--theme-text-secondary)); font-size:.6875rem; }
  .author,.branches { display:inline-flex; align-items:center; gap:.4rem; min-width:0; max-width:100%; }
  code { min-width:0; max-width:16rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; background:rgb(var(--theme-text)/.035); padding:.1rem .3rem; border-radius:.2rem; font-size:.625rem; }
  .branches :global(svg) { flex-shrink:0; }
</style>
