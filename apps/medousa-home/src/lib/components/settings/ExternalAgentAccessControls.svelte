<script lang="ts">
  import { createExternalAgentToken, revokeExternalAgentToken, type ExternalConversation } from "$lib/daemon/externalConversations";
  import type { ExternalAgentScope } from "$lib/types/generated/daemon_api";

  let { conversation, onchange }: { conversation: ExternalConversation; onchange: (value: ExternalConversation) => void } = $props();
  let read = $state(true);
  let work = $state(false);
  let days = $state(30);
  let token = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    const access = conversation.api_access;
    read = access ? access.scopes.includes("read") : true;
    work = access ? access.scopes.includes("work") : false;
  });

  async function issue() {
    if (busy || (!read && !work)) return;
    busy = true;
    error = null;
    token = null;
    try {
      const scopes: ExternalAgentScope[] = [];
      if (read) scopes.push("read");
      if (work) scopes.push("work");
      const result = await createExternalAgentToken(conversation.id, { scopes, expires_in_days: days });
      token = result.token;
      onchange({ ...conversation, api_access: result.access });
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally { busy = false; }
  }

  async function revoke() {
    if (busy) return;
    busy = true;
    error = null;
    try {
      onchange(await revokeExternalAgentToken(conversation.id));
      token = null;
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally { busy = false; }
  }
</script>

<div class="mt-3 space-y-2 border-t border-surface-600 pt-3 text-xs">
  <p class="font-medium">API access</p>
  <p class="workshop-faint">{conversation.provider === "dots" ? "Your dot can use this token with the Medousa CLI on its computer. Slack carries the conversation." : "Instinct uses your workshop’s HTTPS address and this token for curl requests. WhatsApp carries the conversation."}</p>
  {#if conversation.api_access}
    <p>Permissions: {conversation.api_access.scopes.join(", ")} · Expires {new Date(conversation.api_access.expires_at).toLocaleString()}</p>
  {/if}
  <label class="flex items-center gap-2"><input type="checkbox" bind:checked={read} disabled={busy} /> Read workshop notes, calendar, and capability catalog</label>
  <label class="flex items-center gap-2"><input type="checkbox" bind:checked={work} disabled={busy} /> Start background work and read workshop job results</label>
  <label class="flex items-center gap-2">Expires in days <input class="w-20 rounded bg-surface-700 p-1" type="number" min="1" max="90" bind:value={days} disabled={busy} /></label>
  <div class="flex gap-3">
    <button type="button" class="text-content-link" disabled={busy || (!read && !work) || !days || days < 1 || days > 90} onclick={() => void issue()}>{conversation.api_access ? "Replace API token" : "Create API token"}</button>
    {#if conversation.api_access}<button type="button" class="text-content-warning" disabled={busy} onclick={() => void revoke()}>Revoke API token</button>{/if}
  </div>
  {#if token}
    <div class="rounded border border-content-warning p-3">
      <p>Save this token in {conversation.provider === "dots" ? "your dot’s private credential storage" : "Instinct’s private credential storage"}. It is shown once. Replacing it invalidates the previous token.</p>
      <code class="my-2 block break-all select-all">{token}</code>
      <button type="button" class="text-content-link" onclick={() => token = null}>Done</button>
    </div>
  {/if}
  {#if error}<p role="alert" class="text-content-error">{error}</p>{/if}
</div>
