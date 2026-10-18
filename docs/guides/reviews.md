# Review project changes

Open **Reviews** in Medousa's navigation, or find it in Spotlight. The side rail
lists prepared change sets and projects with saved revisions. Search by project
or repository and choose **All**, **Ready**, **Drafts**, or **Reviewed**.

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
- **Approve** records the decision. Its adjacent arrow holds the after-approval
  strategy. Applying approved changes remains a separate
  action, with the existing choice to apply to the source branch or keep the
  reviewed branch. Available actions follow the connected workshop's state.

Code, chat change receipts, and Spotlight Activity open the same review page.
The queue and reviews belong to the connected workshop; opening a review does
not require loading its editable source workspace.
