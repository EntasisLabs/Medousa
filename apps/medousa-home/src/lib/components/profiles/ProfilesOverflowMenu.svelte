<script lang="ts">
  import ProfilesAddProfileDialog from "$lib/components/profiles/ProfilesAddProfileDialog.svelte";
  import { Download, FileText, Upload } from "@lucide/svelte";
  import { openConfigPath } from "$lib/config";
  import {
    exportIdentityMarkdown,
    exportUserProfileBundle,
    importUserProfileBundle,
  } from "$lib/daemon";
  import { toast } from "$lib/stores/toast.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { downloadTextFile } from "$lib/utils/sessionTranscript";
  import { PROFILES_ADD_PROFILE_EVENT } from "$lib/utils/profilesChromeEvents";
  import { onMount } from "svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let sheetOpen = $state(false);
  let importInputEl = $state<HTMLInputElement | null>(null);
  let exportBusy = $state(false);
  let backupBusy = $state(false);
  let status = $state<string | null>(null);

  onMount(() => {
    const onAddProfile = () => {
      sheetOpen = true;
    };
    window.addEventListener(PROFILES_ADD_PROFILE_EVENT, onAddProfile);
    return () => window.removeEventListener(PROFILES_ADD_PROFILE_EVENT, onAddProfile);
  });

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  async function runExportNotes() {
    exportBusy = true;
    status = null;
    try {
      const result = await exportIdentityMarkdown();
      status = "Exported identity notes.";
      try {
        await openConfigPath(result.export_dir);
      } catch (openErr) {
        console.warn("Exported notes but could not open folder", openErr);
      }
      toast.show("Exported identity notes", { durationMs: 1800 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2400 });
    } finally {
      exportBusy = false;
    }
  }

  async function runExportBackup() {
    const profileId = userProfiles.activeProfileId?.trim();
    if (!profileId) {
      toast.show("No active profile to export", { durationMs: 1800 });
      return;
    }
    backupBusy = true;
    status = null;
    try {
      const result = await exportUserProfileBundle({ profileId });
      const label =
        userProfiles.profiles
          .find((p) => p.profile_id === profileId)
          ?.display_name?.trim()
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "") || profileId.slice(0, 8);
      downloadTextFile(
        `medousa-profile-${label}.json`,
        JSON.stringify(result.bundle, null, 2),
        "application/json",
      );
      status = "Exported profile backup.";
      toast.show("Profile backup downloaded", { durationMs: 1800 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2400 });
    } finally {
      backupBusy = false;
    }
  }

  function pickImportBackup() {
    importInputEl?.click();
  }

  async function onImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    backupBusy = true;
    status = null;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as unknown;
      const bundle =
        parsed &&
        typeof parsed === "object" &&
        "bundle" in parsed &&
        (parsed as { bundle: unknown }).bundle
          ? (parsed as { bundle: unknown }).bundle
          : parsed;
      const result = await importUserProfileBundle({ bundle, dryRun: false });
      await userProfiles.load({ suppressRemoteNotice: true });
      status = result.message || "Imported profile backup.";
      toast.show(status, { durationMs: 2200 });
    } catch (err) {
      status = err instanceof Error ? err.message : String(err);
      toast.show(status, { durationMs: 2600 });
    } finally {
      backupBusy = false;
    }
  }
</script>

<input
  bind:this={importInputEl}
  type="file"
  accept="application/json,.json"
  class="hidden"
  onchange={(event) => void onImportFile(event)}
/>

<div class="flex shrink-0 items-center gap-0.5">
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Export profile backup"
    aria-label="Export profile backup"
    disabled={readOnly || backupBusy}
    onclick={() => void runExportBackup()}
  >
    <Download size={15} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Import profile backup"
    aria-label="Import profile backup"
    disabled={readOnly || backupBusy}
    onclick={() => pickImportBackup()}
  >
    <Upload size={15} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn"
    title="Export identity notes"
    aria-label="Export identity notes"
    disabled={readOnly || exportBusy}
    onclick={() => void runExportNotes()}
  >
    <FileText size={15} strokeWidth={1.75} />
  </button>
</div>

<ProfilesAddProfileDialog
  open={sheetOpen}
  {readOnly}
  onClose={() => {
    sheetOpen = false;
  }}
/>

{#if status}
  <p class="sr-only" role="status">{status}</p>
{/if}
