# Vault trash and versions

Two recovery systems: **Trash** (deleted notes) and optional **Versions** (Git-backed snapshots). They are separate — enabling Versions does not replace Trash.

Related: [Vault and notes](guide:vault-notes) · [Troubleshooting](guide:troubleshooting#vault-conflict)

## Trash

1. Open **Library** → dock **Trash**.
2. Each entry shows path and when it was trashed.
3. **Restore** returns the note and reopens it.

Empty state: **Trash is empty.** There is no separate “empty trash” control in the panel today — treat restore as the recovery path.

## Versions (Git)

**Off by default.** When off, you still get normal note conflict checks (Reload / Keep mine).

### Turn on

1. Settings → **Runtime** → **Versions** (Vault versioning subsection), or in-note overflow **Versions…** → start path.
2. Expand **Git on this device** if needed — detect / locate Git, then **Start versioning**.
3. On/Off applies **immediately** (separate from Runtime Save).

Status lines describe Off / On · branch · clean or changed.

### In-note Versions panel

| Control | Meaning |
|---------|---------|
| **Version message** + **Save version** | Named snapshot |
| **Diff vs last** | Patch view of changes |
| **History** → **Restore** | Confirm overwrite of current file |
| **Advanced Git** | Branch, dirty count, worktrees |

The status bar may offer **Open Versions**. Conflict **History** opens this panel when versioning is on.

```callout
tone: warning
title: Restore overwrites
body: Restoring a version replaces the note on disk. Save or snapshot first if you might want your current buffer.
```

### Platform notes

- Versions may be **unsupported** on some builds; Settings shows that clearly.
- Snapshots are **local** to the workshop host — not a multi-device sync product by themselves.
- Mobile may expose a read-only Versions toggle depending on host capability.

## Conflict playbook

| Situation | Prefer |
|-----------|--------|
| You trust the other copy | **Reload** |
| Your buffer is right | **Keep mine** |
| You need history | **History** (Versions on) or Trash if deleted |

Next: [Vault and notes](guide:vault-notes).
