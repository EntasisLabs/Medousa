# Notes and linked vaults

Use the vault picker in **Notes** to choose where you read and create notes.
**Add vault folder…** links another Markdown folder, including an Obsidian vault,
when Medousa and the workshop share the same disk. Files stay in their original
folder. A remote workshop uses its own configured folders.

Changing vaults saves pending note edits first. If saving fails, your draft stays
open and the vault does not change. The list, editor buffers, recent notes, and
last-opened note belong to that vault. Root-relative note tabs are closed after
saving when you switch, so identical filenames cannot reopen another vault's
content. Loose files opened by absolute path are separate from linked vaults.

Both the in-app command center and [Desktop Spotlight](desktop-spotlight.md)
search titles and paths across all linked vaults in the selected workshop. You
do not need to visit Notes first. Results show the vault name and note path.
Previewing a result leaves your current vault alone; opening it selects the
correct vault after saving any pending edits. A failed vault search shows a
notice while results from available vaults remain usable.

Search includes a short text preview and can take a few seconds to reflect an
external edit. If Spotlight asks you to update the workshop engine, rebuild or
update the daemon as well as the app so it can identify each search result's
vault safely.
