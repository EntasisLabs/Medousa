# Versions (vault history)

Medousa can keep optional **Versions** of your vault — everyday language for local Git. Off by default.

## Enable

1. Open **Settings → Versions**.
2. Turn **On**.
3. If Git is missing:
   - **Windows:** Medousa can download portable MinGit into your data folder.
   - **macOS:** install Xcode Command Line Tools (`xcode-select --install`) or Git from git-scm.com.
   - **Linux:** install via your package manager (e.g. `apt install git`).
4. Choose **Start versioning** once to initialize history at the active vault root (adds a sensible `.gitignore` for `.trash/` / `.obsidian/`).

Saves still use normal note conflict checks (`If-Match`). Versions never auto-commit.

## Everyday use

From a note overflow menu → **Versions…**, or the quiet branch label in the note status bar:

- **Save version** — snapshot current changes with a short message
- **History** — browse past versions for the note
- **Restore** — bring a past version back into the working file
- **Diff vs last** — see the patch against the last version

**Advanced Git** (inside the panel) shows branch / dirty count and worktrees. Power users can still use a normal Git client on the vault root.

## Trash

Deleted notes move to `.trash/`. Open **Trash** in the library chrome to restore.
