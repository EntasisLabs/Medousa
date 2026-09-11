<script lang="ts">
  import { onMount } from "svelte";
  import { getUndertaking, startHumanEditingSession, heartbeatLease } from "$lib/forge";
  import { getReviewGitState, runReviewGitAction, type ReviewGitState } from "$lib/chat/reviewGit";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { getCoderExecutionTransport } from "$lib/executionAuthority";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  let { workId, action, ondone, onbusy }: { workId: string; action: "commit" | "pull-request"; ondone: () => void; onbusy: (busy: boolean) => void } = $props();
  let gitState = $state<ReviewGitState | null>(null);
  let selected = $state<string[]>([]);
  let message = $state("");
  let title = $state("");
  let body = $state("");
  let base = $state("");
  let branch = $state("");
  let draft = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let result = $state<{ head?: string; url?: string; existing?: boolean; warning?: string | null } | null>(null);
  let lease: { lease_id: string; generation: number } | null = null;
  let disposed = false;
  const workshop = activeWorkshopId();
  const execution = getCoderExecutionTransport();
  const current = () => !disposed && activeWorkshopId() === workshop && getCoderExecutionTransport() === execution;
  const valid = $derived(Boolean(gitState) && !busy && (action === "commit" ? selected.length > 0 && !!message.trim() : !!title.trim() && !!base.trim() && gitState?.paths.length === 0 && base !== (branch.trim() || gitState?.branch)));
  onMount(() => {
    void load();
    let heartbeatPending = false;
    const timer = setInterval(async () => {
      if (!current() || !lease || heartbeatPending) return;
      heartbeatPending = true;
      try { await heartbeatLease(lease.lease_id, lease.generation); }
      catch { if (current()) { lease = null; gitState = null; error = "Editing session expired. Refresh before continuing."; } }
      finally { heartbeatPending = false; }
    }, 20_000);
    return () => { disposed = true; clearInterval(timer); onbusy(false); };
  });
  async function load() {
    busy = true; onbusy(true); error = null;
    try {
      const item = await getUndertaking(workId);
      if (!current()) return;
      const active = item.active_attempts?.length ? item.active_attempts : [item.active_attempt];
      if (active.length > 1) throw new Error("Wait for all workers to finish before changing Git history.");
      const attempt = item.attempts?.find((a) => a.id === item.active_attempt);
      if (attempt?.lease && attempt.executor?.kind === "human") lease = attempt.lease;
      else {
        if (attempt?.lease) throw new Error("The agent is still working. Stop or finish that turn before changing Git history.");
        const begun = await startHumanEditingSession(workId, item.allowed_actions);
        if (!current()) return;
        lease = begun.lease;
        undertakings.setActiveFromItem(begun.item, { leaseId: lease.lease_id, leaseGeneration: lease.generation, executorKind: "human" });
      }
      const next = await getReviewGitState(workId);
      if (!current()) return;
      gitState = next; selected = [...next.paths]; base = next.base; branch = next.branch;
    } catch (err) { if (current()) error = err instanceof Error ? err.message : String(err); }
    finally { if (current()) { busy = false; onbusy(false); } }
  }
  async function submit() {
    if (!valid || !lease || !gitState || !current()) return;
    busy = true; onbusy(true); error = null;
    try {
      const next = await runReviewGitAction(workId, action, {
        ...lease, snapshot: gitState.snapshot, paths: selected, message: message.trim(),
        title: title.trim(), body, base: base.trim(), draft, branch: branch.trim(),
      });
      if (current()) result = next;
    } catch (err) { if (current()) error = err instanceof Error ? err.message : String(err); }
    finally { if (current()) { busy = false; onbusy(false); } }
  }
</script>
{#if result}
  <div class="git-result" role="status">
    <h3>{action === "commit" ? "Changes committed" : result.existing ? "Pull request already exists" : "Pull request created"}</h3>
    {#if result.warning}<p role="alert">{result.warning}</p>{/if}
    {#if result.head}<p>Commit <code>{result.head.slice(0, 8)}</code></p>{/if}
    {#if result.url && /^https:\/\//.test(result.url)}<a href={result.url} target="_blank" rel="noreferrer">View pull request</a>{/if}
    <button type="button" class="primary" onclick={ondone}>Back to changes</button>
  </div>
{:else}
  <form onsubmit={(event) => { event.preventDefault(); void submit(); }}>
    {#if gitState}
      <p class="branch">Branch <strong>{gitState.branch}</strong> · {gitState.head.slice(0, 8)}</p>
      {#if gitState.can_branch}<label>Branch<input class="input" bind:value={branch} disabled={busy}/></label><p>Enter a new name to create a branch from this checkout.</p>{/if}
      {#if action === "commit"}
        <label>Commit message<textarea class="textarea" bind:value={message} rows="3" disabled={busy} placeholder="Describe these changes"></textarea></label>
        <p>Commits the full current contents of selected files. Other staged files stay staged.</p>
        <fieldset disabled={busy}><legend>{selected.length} of {gitState.paths.length} files selected</legend>
          {#each gitState.paths as path}<label class="file"><input type="checkbox" value={path} bind:group={selected}/><span>{path}</span></label>{/each}
        </fieldset>
        {#if !gitState.paths.length}<p>There are no uncommitted changes.</p>{/if}
      {:else}
        <label>Base branch<input class="input" bind:value={base} disabled={busy}/></label>
        <label>Title<input class="input" bind:value={title} disabled={busy}/></label>
        <label>Description<textarea class="textarea" bind:value={body} rows="5" disabled={busy}></textarea></label>
        <label class="file"><input type="checkbox" bind:checked={draft} disabled={busy}/>Create as draft</label>
        <p>This pushes <strong>{branch || gitState.branch}</strong> to origin and creates a GitHub pull request. GitHub CLI must be signed in on the workshop.</p>
        {#if gitState.paths.length}<p role="status">Commit the remaining changes before creating a PR.</p>{/if}
      {/if}
    {:else if busy}<p role="status">Preparing review…</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p><button type="button" disabled={busy} onclick={() => void load()}>Refresh changes</button>{/if}
    <button type="submit" class="primary" disabled={!valid}>{busy ? "Working…" : action === "commit" ? "Commit selected files" : "Push & create PR"}</button>
  </form>
{/if}
<style>
  form { display: grid; gap: 16px; } label { display: grid; gap: 7px; font-size: 14px; }
  .input, .textarea { width: 100%; font-size: 16px; } .branch { font-size: 13px; opacity: .8; }
  fieldset { border: 0; padding: 0; min-width: 0; } legend { font-size: 13px; opacity: .7; margin-bottom: 8px; }
  .file { display: flex; align-items: center; gap: 12px; min-height: 44px; overflow-wrap: anywhere; }
  input[type="checkbox"] { flex-shrink: 0; width: 18px; height: 18px; }
  .primary { display: block; width: 100%; min-height: 46px; padding: 10px; border-radius: 12px; background: rgb(var(--color-primary-500)); color: white; margin-top: 12px; }
  .primary:disabled { opacity: .4; } .error { color: rgb(var(--theme-error)); } p { font-size: 13px; line-height: 1.5; } .git-result { padding: 24px 0; }
</style>
