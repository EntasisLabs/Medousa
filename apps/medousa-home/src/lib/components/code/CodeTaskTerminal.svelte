<script lang="ts">
  import { onMount } from "svelte";
  import { terminalTheme } from "$lib/terminal/terminalTheme";
  import { withPackagedStyleNonce } from "$lib/security/packagedStyleNonce";
  import type { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";

  let { stdout, stderr, runId, onOpenUrl }: {
    stdout: string; stderr: string; runId: string;
    onOpenUrl: (url: string) => void;
  } = $props();
  let host: HTMLDivElement;
  let failure = $state("");
  let terminal = $state.raw<Terminal | null>(null);
  let previousOut = "", previousErr = "", previousRun = "";

  $effect(() => {
    if (!terminal) return;
    const reset = runId !== previousRun || !stdout.startsWith(previousOut) || !stderr.startsWith(previousErr);
    const atBottom = terminal.buffer.active.viewportY >= terminal.buffer.active.baseY;
    if (reset) {
      terminal.reset(); terminal.write("\x1b[?25l"); previousOut = ""; previousErr = "";
    }
    // Feed raw terminal data to xterm, including partial escape sequences. Never
    // turn task output (or OSC hyperlinks) into HTML or native terminal commands.
    const out = stdout.slice(previousOut.length), err = stderr.slice(previousErr.length);
    if (out) terminal.write(out);
    if (err) terminal.write(err);
    previousOut = stdout; previousErr = stderr; previousRun = runId;
    if (atBottom && (out || err)) terminal.write("", () => terminal?.scrollToBottom());
  });

  onMount(() => {
    let disposed = false;
    let observer: ResizeObserver | undefined;
    void (async () => {
      const [{ Terminal }, { FitAddon }, { WebLinksAddon }] = await Promise.all([
        import("@xterm/xterm"), import("@xterm/addon-fit"), import("@xterm/addon-web-links"),
      ]);
      if (disposed) return;
      const open = (url: string) => {
        try { if (["http:", "https:"].includes(new URL(url).protocol)) onOpenUrl(url); } catch { /* not a web URL */ }
      };
      const term = new Terminal({
        disableStdin: true, convertEol: true, cursorBlink: false,
        fontSize: 12, fontFamily: 'ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace',
        scrollback: 10000, allowProposedApi: false,
        theme: terminalTheme, minimumContrastRatio: 4.5,
        linkHandler: { activate: (_event, text) => open(text) },
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.loadAddon(new WebLinksAddon((_event, url) => open(url)));
      withPackagedStyleNonce(() => term.open(host));
      term.write("\x1b[?25l");
      observer = new ResizeObserver(() => { if (host.clientWidth && host.clientHeight) fit.fit(); });
      observer.observe(host);
      fit.fit(); terminal = term;
    })().catch(error => { if (!disposed) failure = error instanceof Error ? error.message : String(error); });
    return () => { disposed = true; observer?.disconnect(); terminal?.dispose(); terminal = null; };
  });
</script>

{#if failure}<p role="alert">Couldn’t load task output: {failure}</p>{/if}
<div bind:this={host} class="task-terminal min-h-0 flex-1 overflow-hidden px-2.5 py-1.5" role="region" aria-label="Task output"></div>

<style>
  .task-terminal :global(.xterm) { height: 100%; }
  .task-terminal :global(.xterm-viewport) { overscroll-behavior: contain; }
</style>
