<script lang="ts">
  import { FolderGit2, GitBranch, SquareTerminal, FileCode2 } from "@lucide/svelte";
  import { haptic } from "$lib/haptics";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import type { MobileCodeSurface } from "$lib/utils/mobileCodeLanding";

  interface Props {
    /** Labeled tab bar, or icon chips for the Terminal key row. */
    variant?: "labeled" | "icons";
  }

  let { variant = "labeled" }: Props = $props();

  const rooms: Array<{ id: MobileCodeSurface; label: string; icon: typeof FileCode2 }> = [
    { id: "files", label: "Files", icon: FolderGit2 },
    { id: "editor", label: "Editor", icon: FileCode2 },
    { id: "terminal", label: "Terminal", icon: SquareTerminal },
    { id: "changes", label: "Changes", icon: GitBranch },
  ];

  const active = $derived(mobileCodeWorkspaceState.surface);

  function select(id: MobileCodeSurface) {
    if (id === active) return;
    haptic("light");
    mobileCodeWorkspaceState.switchRoom(id);
  }
</script>

<nav
  class={variant === "icons" ? "mobile-code-switcher-icons" : "mobile-code-switcher"}
  aria-label="Project rooms"
>
  {#each rooms as room (room.id)}
    {@const Icon = room.icon}
    <button
      type="button"
      class={variant === "icons" ? "mobile-code-key-room" : "mobile-code-switcher-btn"}
      class:mobile-code-switcher-btn-active={variant === "labeled" && active === room.id}
      class:mobile-code-key-room-active={variant === "icons" && active === room.id}
      aria-current={active === room.id ? "page" : undefined}
      aria-label={room.label}
      onclick={() => select(room.id)}
    >
      <Icon size={18} strokeWidth={1.75} />
      {#if variant === "labeled"}
        <span>{room.label}</span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  .mobile-code-switcher {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    flex-shrink: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.92);
  }

  .mobile-code-switcher-btn {
    display: flex;
    min-height: 2.75rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.125rem;
    padding: 0.35rem 0.25rem;
    color: rgb(var(--color-content-quiet));
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .mobile-code-switcher-btn-active {
    color: rgb(var(--color-content-link));
  }

  .mobile-code-switcher-icons {
    display: flex;
    flex-shrink: 0;
    gap: 0.25rem;
  }

  .mobile-code-key-room {
    display: inline-flex;
    min-width: 2.75rem;
    min-height: 2.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-content-quiet));
  }

  .mobile-code-key-room-active {
    border-color: rgb(var(--color-content-link) / 0.55);
    color: rgb(var(--color-content-link));
  }
</style>
