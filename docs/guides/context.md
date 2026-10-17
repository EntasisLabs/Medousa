# Browse saved session memory

Open **Context** to read saved memory from the selected workshop's sessions:
moments, summaries, checkpoints, decisions, and handoffs. **Browse** is the
default lens. The left rail holds the searchable memory list; the main pane
shows the selected memory. Titles use short labels such as Verification or
Checkpoint. Longer memories include an excerpt in the rail. The reader shows
the full saved summary and content, with sections for decisions and rationale,
observations, completed or remaining work, and blockers when those fields were
saved. Unfamiliar content fields remain readable rather than disappearing.
Record details preserves the original text and identifiers.
Search matches loaded summaries and source sessions, including text beyond the
short list preview.

**Working context** identifies the recorded project, branch, and commit when
present. These describe the saved moment, not the current repository state.
**Evidence and references** lists saved evidence identifiers, files, and symbols.
References are not proof that a check passed. Automatic tool checkpoints show
the recorded execution outcome separately; completion alone is not a test verdict.
**Connected memory** shows explicit saved relationships, such as derived-from or
supersedes. Available targets open in Context; unavailable references remain
inspectable without a broken navigation action.

**Open source session** is available when the recorded author session or memory
session is in the loaded chat list. **Record details** reveals the original record and its identifier.
**Related notes** shows compact links to notes explicitly associated with the
source session. Selecting a link opens the document in **Notes**; Context does
not fetch or render note bodies.

Switch to **Map** to explore the same saved session memory. Switching lenses
keeps the selected memory or session. Click a memory or session to focus it,
double-click a session to expand its moments, or switch back to **Browse** to
read the selected memory. Related note nodes open directly in Notes.
Connections include session membership, moment sequence, note associations,
shared note tags, and sessions close in time. A connection does not mean that
an agent received the item in a particular turn.

Context initially loads up to 200 recent moments. **Show more saved memory**
reveals more rows from that loaded list. **Refresh Context** updates the catalog
and related-link metadata. Related links currently use a bounded set of up to
200 note metadata records; notes with no recorded association to a matching
source session are not included. Map displays at most 30 moments per session and 80
related notes.

Your profile, preferences, people, and personal facts remain in **You**. Documents
are read and edited in **Notes**. Ongoing work and items needing attention remain
in Spotlight's **Activity** lens.

On phones, **Saved memory** switches between the list and the full reader while
keeping the selected memory.

The reader interprets the saved record for display without changing the memory.
Records it cannot interpret retain their summary and original-record view.
