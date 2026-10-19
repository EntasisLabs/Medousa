# Review project changes

Open **Reviews** in Medousa's navigation, or find it in Spotlight. The side rail
groups reviews into collapsible **Forge** and provider sections, such as **GitHub**.
Both sources stay visible together. Search by project or repository and choose
**All**, **Active**, or **Reviewed**; Active includes open and draft reviews.

Select a review to open its own tab in the main pane. The list stays in the rail
so you can switch reviews directly. **Open project** in project actions opens its
working copy in Code.

- **Summary** opens first. It shows changed areas, recorded file intents, and the
  saved check result for the selected revision. Missing check results are explicit.
  Choose a changed area to show only its files. The original request is available
  as a disclosure below the change summary. Checks, change totals, target, and
  risk sit alongside the reading column.
- **Changes** uses the same file diffs, inline comments, and source-file links
  available from Code. Diffs load as files expand. Use the directory selector to
  narrow the list, or choose **All files** to clear it.
- The revision picker selects a prepared attempt. Navigation remembers that
  revision, directory filter, selected lens, and reading position for the current app
  session. A later attempt does not silently replace the one being read.
- **Review notes** opens comments on demand. Add comments from a diff line, or
  add a decision note through project actions. **Request changes** uses that
  feedback to start the next attempt through the configured coding runtime.
- Use **Project actions** for history, pull-request links, and editing tools.
- **Review → Approve changes** records the decision. The same menu holds the
  after-approval strategy. Applying approved changes remains a separate
  action, with the existing choice to apply to the source branch or keep the
  reviewed branch. Available actions follow the connected workshop's state.

Code, chat change receipts, and Spotlight Activity open the same review page.
The queue and reviews belong to the connected workshop; opening a review does
not require loading its editable source workspace.

## Repository-hosted reviews

For a local workshop, install **GitHub reviews** in **Settings → Packages**.
For a remote workshop, install the package on that daemon's machine with
`medousa pull review-github`.

Open **Settings → Accounts** and choose **Sign in with GitHub**. Medousa opens
your browser and shows a short authorization code. Enter that code on GitHub and
approve access. Medousa finishes connecting automatically. The connected
account is selected for reviews automatically. With several saved accounts, use
**Use for reviews** in Settings to choose one. Sign-in and account management stay
in Settings, separate from the review list.

Tokens stay in the workshop's keychain/keyring. Native accounts do not require
GitHub CLI or a checkout on the device displaying Medousa. If no account is selected,
the package can reuse an existing `gh auth login` on the workshop machine.
Disconnecting an account in Settings removes its saved credentials from that
workshop; it does not revoke the app on GitHub.

The GitHub package includes Medousa's sign-in registration. You do not need to
register an app, configure a callback, enter a client ID, or host an auth service.
GitHub shows the requested `repo` and `read:user` access in the browser. Use
**Copy code** or **Open sign-in page** if needed. **Cancel** stops waiting;
**Retry connection** resumes an interrupted check without requesting a new code.

The Reviews rail loads your account's **Needs your review** inbox automatically.
Use the inbox selector inside the GitHub section for **Your pull requests** or
**Participating**. Forge reviews remain in their own section while the hosted inbox changes. Your PRs and participating PRs include closed and merged
items; review requests show open PRs. The existing state filters narrow the loaded
results. Refresh reloads the selected inbox or repository without switching it.

You can also paste an HTTPS pull request URL and press Enter, or enter
`owner/repository` to list its pull requests. When several provider packages are
installed, select the provider. Hosted reviews open in separate tabs with
**Summary**, **Changes**, **Discussion**, and **Checks**. File patches load when
expanded; only one patch is retained by the page. Load-more controls fetch
additional pages explicitly. The rail keeps up to 500 metadata records for the
current workshop and app session, without retaining review descriptions or diffs.

Use **Leave a comment…** on Summary, or **Pull request actions → Comment**, for a conversation comment.
The composed Summary places the description beside reviewers, current checks, and merge status.
Checks load when the review opens; file patches and discussion still load on demand.
Descriptions, discussion, and check output use Liquid Markdown’s reading presentation,
including styled disclosure sections, tables, code controls, and linked image badges. In **Changes**, line comments are
staged locally until you choose **Review** and submit Comment, Approve, or Request
changes. Inline threads in **Discussion** have reply composers. Pending inline
comments remain across refreshes in the open tab; if the head changes, remove the
old pending comments before reviewing the new revision. Draft text and pending comments survive switching or reopening tabs within this
Medousa window, scoped to the workshop, PR, and account; closing the window ends
that temporary draft storage.

**Pull request actions** includes editing the title/description, requesting reviewers by
username, draft/ready state, and close/reopen. **Merge…** lets you select a merge
method enabled by the repository and confirm the displayed commit. GitHub still
enforces permissions, required reviews, checks, and branch protections. Available
actions depend on ownership, repository access, and PR state.

**Checks** includes check runs, commit statuses, and workflows. Open **Details**
for check output and annotations or workflow job steps. Repository writers can
rerun all/failed workflow jobs or cancel a running workflow. Full logs, artifacts,
and additional annotations remain available with **Open check**. Refresh to fetch
new CI state. Large collections use explicit load-more controls.

Writes are never automatically retried. If a response is interrupted, the draft
stays in the form; refresh and check the discussion/state before submitting again.

**Open on …** opens the provider's page in your default browser. Forge's **Share** and chat's **Create pull request** use the
selected account and provider package to publish a branch's review. Repeating the
action reuses an existing open review for those head/base branches; Forge sharing
also updates its description. Git branch pushes still use the workshop's Git/SSH
credentials. If publication is not confirmed, retry looks for the existing review
before attempting a create. Local Forge approval and apply actions
continue to operate on local prepared changes. External reviews keep their own
identities and do not create Forge tasks.

If a new commit arrives while reading a review, refreshing selects the current
revision. Loading files from a changed revision asks you to refresh rather than
mixing new patches into the displayed revision. GitHub may omit patches for
binary or large files and limits file listings to 3,000 files; use the provider
page when a text patch is unavailable.
