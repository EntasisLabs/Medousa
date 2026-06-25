<script lang="ts">
  import { Check } from "@lucide/svelte";

  interface Props {
    onLaunch: () => void;
    onModify: () => void;
    onReleaseNotes: () => void;
  }

  let { onLaunch, onModify, onReleaseNotes }: Props = $props();
</script>

<section class="complete-screen screen-center">
  <div class="success-icon" aria-hidden="true">
    <Check size={32} strokeWidth={2.5} />
  </div>

  <header class="screen-header">
    <h1>You're all set</h1>
    <p class="lead">First-run setup will configure your brain and devices.</p>
  </header>

  <button class="btn-launch" type="button" onclick={onLaunch}>Launch Medousa</button>

  <div class="complete-links">
    <button class="complete-link" type="button" onclick={onModify}>Modify installation</button>
    <button class="complete-link" type="button" onclick={onReleaseNotes}>What's new</button>
  </div>
</section>
