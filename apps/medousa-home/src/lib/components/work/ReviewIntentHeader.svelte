<script lang="ts">
  import type { ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    /** Panel title — never reprint as the outcome. */
    projectTitle?: string | null;
  }

  let { review, projectTitle = null }: Props = $props();

  const outcome = $derived.by(() => {
    const title = projectTitle?.trim() ?? "";
    const brief = review.synthesis.outcome?.trim() ?? "";
    const usableBrief =
      brief &&
      (!title || brief.toLowerCase() !== title.toLowerCase())
        ? brief
        : "";
    if (usableBrief) return usableBrief;

    const intents = review.changed_files
      .flatMap((file) => file.intents ?? [])
      .filter(Boolean);
    const unique = [...new Set(intents)];
    if (unique.length === 1) return unique[0]!;
    if (unique.length > 1) {
      return `${unique.length} edits across ${review.changed_files.length} ${review.changed_files.length === 1 ? "file" : "files"}`;
    }
    const n = review.changed_files.length;
    if (n === 0) return "No file changes in this revision";
    return `Changes in ${n} ${n === 1 ? "file" : "files"}`;
  });
</script>

<header class="intent-header" aria-label="Review summary">
  <p class="intent-outcome">{outcome}</p>
</header>

<style>
  .intent-header {
    margin-bottom: 0.75rem;
  }

  .intent-outcome {
    margin: 0;
    max-width: 42rem;
    font-size: 0.9375rem;
    font-weight: 550;
    line-height: 1.45;
    color: rgb(var(--theme-text));
    letter-spacing: -0.01em;
  }
</style>
