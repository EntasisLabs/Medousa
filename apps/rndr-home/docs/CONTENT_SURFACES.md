# Desktop content surfaces — bounded Notes summary browser

Admission baseline: `a4ea4400482a27183340f738f0d961d571b94157` on
`medousa/runtime-ui-fixes`, verified clean in the Forge-owned isolated undertaking
`work-09765685d470952709f6814b15d073c0`. Prior shell, nine-section Settings and
Activity implementations are present. The unused default-main scaffold
`work-6500d9d44a0c5d684e76a4c900982474` was not used.

This is the **current-status matrix** following
[SHELL_ACTIVITY_PARITY.md](SHELL_ACTIVITY_PARITY.md). That prior integrated report
and its evidence are preserved. This worker does not accept or integrate its own
work. No push, deployment, daemon/framework change or production-ready claim.

## Audited scope and why this subset

Production references (under `apps/medousa-home/src/lib/`):
`stores/vault.svelte.ts`, `config/vaultSpaces.ts`, `utils/vaultNoteKind.ts`,
`daemon/workspace.ts`, plus the existing shell/Settings references in the previous
matrix. Library is the production grouping, **not a nineteenth preview rail door**.
The original preview rail calls this destination Notes.

- **Selected: Notes summary browsing.** The SDK already offers
  `vault().list_notes(VaultNotesQuery)` using
  **GET `/v1/vault/notes?limit=100`**. Types are `VaultNotesListResponse` and
  `VaultNoteSummary` in `crates/medousa-types/src/daemon_api.rs`. This is a current
  daemon-vault inventory, not a disk picker or the chat session's attachments.
- **Files: deferred.** The audited SDK has path-based note/file reads, not an
  authoritative generic file inventory with this preview's locality/host adapter.
  A path input or locally walking the daemon's disk would not be a safe substitute.
- **Artifacts: deferred, not unavailable in the backend.** SDK `runtime.rs` and
  production `daemon/workspace.ts` have `artifact_list_ui` / `artifact_fetch`,
  POST-based request envelopes including session selection. An independent scoped
  read adapter and session/workshop isolation tests must precede that port. This
  increment does not relax the closed GET-only worker or fetch/render HTML.
- **Full Library: deferred.** Folder/group trees, custom spaces, multi-root cache
  identity, note bodies, data-first kinds, attachments and editors are not claimed.
  The backend has more note-reading support than this conservative UI exposes.

## Notes behavior and boundaries

**Usable bounded preview, not full completion:** list → summary detail → Back,
local case-insensitive filtering of displayed titles/paths/timestamps, explicit
refresh, keyboard activation/focus restoration and responsive layout. Back retains
the filter and results viewport; focus visibility can reveal the origin row.
Refresh clears positional selection, deduplicates pending reads and serial-fences
stale replies. Switching destinations clears selection/filter; snapshots remain
window-local, resource-keyed and never persisted or attached to a chat session.
There is no workshop selector or root switching; changing workshop requires a new
explicit launch, not cache reuse across authorities.

Only **title, inert vault-relative `.md` path and modified UTC** are projected.
Labels are bounded to 240 characters and plain native `Label`/`Button` text, not
Markdown/HTML, URL openers or executable content. Control and bidi/invisible format
characters are removed from titles. Paths reject traversal, absolute/URL/drive,
backslash, empty/dot segments, controls/bidi, percent-encoding and query/fragment
syntax; duplicate identifiers or unsafe paths make the page incompatible without
echoing them. Paths are never used for a second HTTP read or filesystem operation.

The default production Library hides system/developer and Ask/link-test notes.
This preview is intentionally stricter: hidden paths, `system/`, `bugs/`,
`medousa-daemon`, Ask/link-test and chat/session/profile-tagged notes are excluded,
without an opt-in. Tags are inspected only for exclusion and never projected.
`src/vault/semantic_tags.rs` shows that profile/session tags can be applied
automatically to ordinary authored notes. Therefore this deliberately conservative
preview can omit legitimate notes, especially on a non-default profile; it is not
a complete usable inventory for every profile. Safe profile-scoped browsing needs
an explicit identity-aware contract rather than treating tag presence as a grant.
Bodies, backlinks, arbitrary kind strings, cursors, diagnostics and extra response
fields are not retained in the presentation. Note titles themselves are user
content; this is not a general secret detector or data-loss-prevention system.

The query and projection are capped at **100 summaries before exclusions**.
Only the first page is loaded; cursors are not followed. Every state says it is
not the whole vault, even for older adapters without pagination metadata.
`reset_required` fails incompatible; Refresh retries the head page. No claim that
an absent cursor proves completeness. Empty means **no visible notes in this
first page**, not empty vault. Filters cannot search omitted pages or note bodies.
Loading, offline/not-loaded, retained offline snapshot, empty, no-match, permission,
unavailable, disconnected and unsupported/incompatible states remain distinct.
The existing SDK erases structured HTTP status; HTTP 404 is currently unavailable,
not a fabricated positive result or a precise capability diagnosis.

This reuses the existing local-loopback client/credential boundary and 12-second
read deadline. No live daemon, credential store, external provider or filesystem
content was contacted/read in this implementation verification: only ephemeral
loopback test servers and explicitly synthetic offline exports. No new authority,
content mutation, execution control, permission prompt or live success fallback.

## Original 18 rail destinations — exact current status

**5 of 18 have usable preview views (all bounded/partial); 13 of 18 remain
deferred. None is asserted production-complete.** A deferred explanation is not a
usable destination. Settings subpages and Library grouping do not inflate totals.

| # | Original door | Current preview classification | Still missing / boundary |
|---|---|---|---|
| 1 | Chat | Existing bounded preview preserved | Send/Restore/New/Recent/Reload remain denied; native input proof and wider parity pending |
| 2 | Code | Deferred | Project inventory/detail/editor and governed actions |
| 3 | Reviews | Deferred | Provider/project review browsing and native sealed-action UX |
| 4 | Notes | **New usable bounded summary browser** | First page only, no bodies/pagination/folder tree/roots/editor; not full Library |
| 5 | Files | Deferred | Authoritative inventory and workshop-locality/host adapter; no arbitrary path UI |
| 6 | Artifacts | Deferred | Scoped metadata/read adapter, session isolation and safe text-only details; no raw HTML |
| 7 | Calendar | Deferred | Authoritative event reads, timezone and permission boundaries |
| 8 | Web | Deferred | Existing browser capability/grant and navigation boundary |
| 9 | Context | Deferred | Safe bounded context graph projection; no raw prompt/diagnostic leakage |
| 10 | Peers | Deferred | Trusted participant inventory, locality and pairing UI |
| 11 | Agents | Deferred | Actual roster/detail, not the Settings mode registry |
| 12 | Activity | Existing usable bounded read-only snapshot | Up to 100 workspace work cards, not complete jobs ledger/unified feed/subscriptions |
| 13 | Scripts | Deferred | Definition browser, safe source projection; no run/editor here |
| 14 | Flows | Deferred | Definition/detail browser and governed execution boundary |
| 15 | Schedules | Existing usable bounded read-only snapshot | Runs/delivery/management/editing deferred |
| 16 | Create | Deferred | Authoring/presentation and explicit acceptance workflows |
| 17 | Settings | Existing usable local/read-only subsets | Nine routes, not complete editable settings; accounts/pairing/channels/packages/connection switching remain deferred |
| 18 | You | Deferred | Identity/profile presentation, ownership and persistence |

Native pane/tab/titlebar/workspace fidelity remains partial/deferred, unchanged.
All five sensitive chat actions remain denied; no rndr investigation or repair is
part of this slice. The prior chooser-documentation and test-helper limitations
remain historical evidence, not expanded qualification.

## Verification, remaining human checks and next priorities

See [exact checks, sample paths and inspected-pixel ledger](content-evidence/REPORT.md).
Synthetic checks are not native device/live-service/IME/accessibility proof.

1. **Next content increment:** bounded Notes pagination with generation/root identity
   preserved, followed by an explicitly requested, bounded plain-text note body
   reader if its content/privacy contract is accepted. Avoid treating a larger
   inventory as a complete Library editor.
2. **Artifacts metadata browsing:** audit the existing session/workshop-scoped POST
   read envelope and implement a closed typed adapter, never a generic command
   channel or raw HTML renderer. Keep unknown/unsupported states honest.
3. **Calendar or Scripts definition browsing:** choose based on audited existing
   read contracts; no execution or content writes. Files needs locality/inventory
   support before a generic file browser can be honest.
4. **Human qualification:** actual daemon identity/root and real first-page
   ordering/timestamps; permission/offline/reset/empty behavior on an explicitly
   authorized service; physical keyboard/pointer/scroll, screen-reader bridge,
   focus on resize, scaling and native windows across target devices. Physical IME
   and five denied actions remain on the separate input-safety track.

The sender retains independent review, corrections and native integration.
