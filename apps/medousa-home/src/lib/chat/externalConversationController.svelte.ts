import { onMount } from "svelte";
import { externalConversationBinding } from "$lib/utils/externalConversationSession";
import {
  listExternalConversations,
  sendExternalConversationMessage,
  type ExternalConversation,
  type ExternalProvider,
} from "$lib/daemon/externalConversations";
import { getExternalConversationSelection, setExternalConversationSelection } from "$lib/utils/externalConversationSelection";
import { externalConversationMessages } from "$lib/utils/externalConversationMessages";

export function createExternalConversationController(input: {
  provider: () => ExternalProvider | null;
  sessionId: () => string;
  scope: () => string | null;
  offline: () => boolean;
  visible: () => boolean;
}) {
  let conversations = $state<ExternalConversation[]>([]);
  let selectedId = $state<string | null>(null);
  let busy = $state(false);
  let loading = $state(false);
  let loaded = false;
  let refreshSeq = 0;
  let error = $state<string | null>(null);
  const choices = $derived(conversations.filter((item) => item.provider === input.provider()));
  const selected = $derived(choices.find((item) => item.id === selectedId) ?? null);
  const messages = $derived(externalConversationMessages(selected));

  function select(id: string) {
    const provider = input.provider();
    if (!provider) return;
    const binding = externalConversationBinding(input.sessionId());
    const retained = getExternalConversationSelection(input.sessionId(), provider);
    if ((binding && binding.id !== id) || (retained && retained !== id)) return;
    selectedId = id;
    setExternalConversationSelection(input.sessionId(), provider, id);
    error = null;
  }

  async function refresh() {
    const provider = input.provider();
    if (!provider || input.offline()) return;
    const sessionId = input.sessionId();
    const scope = input.scope();
    const sequence = ++refreshSeq;
    if (!loaded) loading = true;
    try {
      const next = await listExternalConversations();
      if (sequence !== refreshSeq) return;
      if (input.provider() !== provider || input.sessionId() !== sessionId || input.scope() !== scope) return;
      conversations = next;
      const available = next.filter((item) => item.provider === provider);
      const desired = selectedId ?? getExternalConversationSelection(sessionId, provider);
      if (desired) {
        selectedId = desired;
      } else if (available.length === 1) {
        select(available[0].id);
      } else {
        selectedId = null;
      }
      error = null;
    } catch (cause) {
      if (sequence === refreshSeq) error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (sequence === refreshSeq) {
        loaded = true;
        loading = false;
      }
    }
  }

  async function send(text: string, hasExtraInputs: boolean) {
    if (!selected) {
      error = "Open a connected agent from Sessions before sending.";
      throw new Error(error);
    }
    if (hasExtraInputs) {
      error = "External agent conversations currently accept text only. Remove attachments and tools before sending.";
      throw new Error(error);
    }
    if (busy) throw new Error("A message is still sending.");
    busy = true;
    error = null;
    const scope = input.scope();
    const sessionId = input.sessionId();
    const provider = input.provider();
    const current = () => input.scope() === scope && input.sessionId() === sessionId && input.provider() === provider;
    try {
      const updated = await sendExternalConversationMessage(selected.id, text, crypto.randomUUID());
      if (current()) conversations = conversations.map((item) => item.id === updated.id ? updated : item);
    } catch (cause) {
      if (current()) {
        await refresh();
        if (current()) error = cause instanceof Error ? cause.message : String(cause);
      }
      throw cause;
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    const provider = input.provider();
    const sessionId = input.sessionId();
    input.scope();
    refreshSeq += 1;
    conversations = [];
    loading = false;
    error = null;
    loaded = false;
    selectedId = provider ? getExternalConversationSelection(sessionId, provider) : null;
    if (provider) void refresh();
  });

  onMount(() => {
    const changed = () => void refresh();
    window.addEventListener("medousa-external-conversation-changed", changed);
    const timer = window.setInterval(() => {
      if (input.visible() && input.provider() && document.visibilityState === "visible" && !busy) changed();
    }, 3000);
    return () => {
      window.removeEventListener("medousa-external-conversation-changed", changed);
      window.clearInterval(timer);
    };
  });

  return {
    get conversations() { return conversations; },
    get choices() { return choices; },
    get selectedId() { return selectedId; },
    get selected() { return selected; },
    get messages() { return messages; },
    get busy() { return busy; },
    get loading() { return loading; },
    get error() { return error; },
    select,
    refresh,
    send,
  };
}
