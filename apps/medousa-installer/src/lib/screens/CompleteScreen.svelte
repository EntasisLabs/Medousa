<script lang="ts">
  interface Props {
    onLaunch: () => void;
    onModify: () => void;
    onReleaseNotes: () => void;
  }

  let { onLaunch, onModify, onReleaseNotes }: Props = $props();
</script>

<section class="screen">
  <header class="screen-header">
    <h1>Installation complete</h1>
    <p class="lead">Medousa is ready. The first-run wizard will help you configure your brain and pairing.</p>
  </header>

  <footer class="footer-actions">
    <button class="primary" type="button" onclick={onLaunch}>Launch Medousa</button>
    <button class="secondary" type="button" onclick={onModify}>Modify installation</button>
    <button class="secondary" type="button" onclick={onReleaseNotes}>Release notes</button>
  </footer>
</section>
