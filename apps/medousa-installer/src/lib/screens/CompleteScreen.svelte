<script lang="ts">
  import { Check } from "@lucide/svelte";

  interface Props {
    onLaunch: () => void;
    onManage: () => void;
    onReleaseNotes: () => void;
  }

  let { onLaunch, onManage, onReleaseNotes }: Props = $props();
</script>

<section class="complete-screen screen-center">
  <div class="success-icon" aria-hidden="true">
    <Check size={32} strokeWidth={2.5} />
  </div>

  <header class="screen-header">
    <h1>You're all set</h1>
    <p class="lead">Medousa is installed. Open the app to finish setup — connect your accounts, pick a brain, and you're ready.</p>
  </header>

  <button class="btn-launch" type="button" onclick={onLaunch}>Open Medousa</button>

  <div class="complete-links">
    <button class="complete-link" type="button" onclick={onManage}>Add or remove components</button>
    <button class="complete-link" type="button" onclick={onReleaseNotes}>What's new</button>
  </div>
</section>
