<script lang="ts">
  import { browserBookmarks } from "$lib/stores/browserBookmarks.svelte";
  import { browserHistory } from "$lib/stores/browserHistory.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import {
    faviconUrlForSite,
    hostnameFromUrl,
    tabDisplayLabel,
  } from "$lib/utils/browserFavicon";

  const bookmarks = $derived(browserBookmarks.list().slice(0, 8));
  const recents = $derived(browserHistory.recent(8));

  async function openUrl(url: string) {
    await humanBrowser.navigate(url);
  }
</script>

<div class="browser-start-page">
  <div class="browser-start-page-inner">
    {#if recents.length > 0}
      <section class="browser-start-section">
        <h2 class="browser-start-section-label">Recents</h2>
        <ul class="browser-start-recents">
          {#each recents as entry (entry.url + entry.visitedAt)}
            <li>
              <button
                type="button"
                class="browser-start-recent-row"
                onclick={() => void openUrl(entry.url)}
              >
                <img
                  src={faviconUrlForSite(entry.url, 32)}
                  alt=""
                  class="browser-start-favicon"
                  width="28"
                  height="28"
                />
                <span class="min-w-0 flex-1 text-left">
                  <span class="browser-start-recent-title">
                    {tabDisplayLabel(entry.title, entry.url)}
                  </span>
                  <span class="browser-start-recent-host">{hostnameFromUrl(entry.url)}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if bookmarks.length > 0}
      <section class="browser-start-section">
        <h2 class="browser-start-section-label">Saved places</h2>
        <div class="browser-start-saved-grid">
          {#each bookmarks as entry (entry.url)}
            <button
              type="button"
              class="browser-start-saved-tile"
              title={entry.title}
              onclick={() => void openUrl(entry.url)}
            >
              <img
                src={faviconUrlForSite(entry.url, 32)}
                alt=""
                class="browser-start-saved-icon"
                width="32"
                height="32"
              />
              <span class="browser-start-saved-label">{tabDisplayLabel(entry.title, entry.url)}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  </div>
</div>
