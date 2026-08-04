# Medousa for Neovim

Medousa for Neovim is a keyboard-first coding room. It opens as a transient
floating window, carries the current buffer/selection/diagnostics into the
conversation, and returns focus to your prompt after each turn. It is a
focused extension of coding rather than a second persistent chat pane.

## Install a development checkout

The plugin currently ships as a Lua runtime directory. With `lazy.nvim`, point
the plugin at the repository checkout:

```lua
{
  dir = "/path/to/Medousa/integrations/neovim",
  config = function()
    require("medousa").setup()
  end,
}
```

Or add `integrations/neovim` to Neovim's runtime path and call
`require("medousa").setup()` from your configuration.

Medousa connects to `http://127.0.0.1:7419` by default. Configure another
workshop or token without changing the plugin:

```lua
require("medousa").setup({
  endpoint = "http://127.0.0.1:7419",
  token = vim.env.MEDOUSA_TOKEN,
})
```

The token is read from configuration or `MEDOUSA_TOKEN`; it is not logged.
The active daemon session id is stored in Neovim's state directory so the
coding room can resume after restarting the editor.

Medousa for Neovim requires Neovim 0.10 or newer and `curl`. Telescope is
detected automatically when installed, but it is not required.

## Use the coding room

Default normal-mode mappings are:

- `<leader>mc` toggles the floating coding room.
- `<leader>ma` opens the composer; in visual mode it keeps that range as context.
- `<leader>me` asks about the visual selection, or pre-fills a buffer question.
- `<leader>mf` asks about current diagnostics, or pre-fills a buffer review.
- `<leader>mo{motion}` captures a motion as context before opening the composer.
- `<leader>ms` opens conversation history.
- `<leader>mm` switches the conversation between General and Coder.
- `<leader>mp` chooses, creates, or detaches the conversation's governed project.

The composer is an ordinary editable Neovim buffer: Enter creates a new line,
normal-mode Enter or Ctrl-S sends, and closing the room keeps the unfinished
draft for that conversation. Inside the transcript, `a` returns to the
composer, `q` returns to code, `c` cancels an active response, `r` retries,
`y` copies the settled answer, `Y` copies the code fence under the cursor,
`Tab` expands tool activity, and `[c`/`]c` navigate code fences. Pressing `A`
inside a fence previews that block directly.

The commands `MedousaToggle`, `MedousaAsk`, `MedousaExplain`, `MedousaFix`,
`MedousaApply`, `MedousaNew`, `MedousaCancel`, `MedousaSessions`,
`MedousaMode`, `MedousaProject`, `MedousaRename`, `MedousaDelete`,
`MedousaDiagnostics`, `MedousaAttention`, and `MedousaStatus`
provide the same paths for custom mappings. `:MedousaAsk` accepts an optional
prompt and Ex range, for example `:'<,'>MedousaAsk simplify this`.

Streaming replies show concise thinking/tool/recovery status and reconnect by
event sequence when the connection is interrupted. The transcript does not
force-scroll while you are reading earlier work. Closing the window does not
discard the daemon conversation.

Budget and tool-permission requests use a focused Approve/Deny picker. Closing
the picker leaves the request pending instead of guessing; run
`:MedousaAttention` to reopen it.

## Conversations

`<leader>ms`, `s` inside the room, or `:MedousaSessions` opens the same daemon
conversation history used by Medousa and VS Code. Telescope provides the
picker when available; otherwise the plugin uses `vim.ui.select`.

Use `R`/`:MedousaRename` to name the active conversation and
`D`/`:MedousaDelete` to delete it after confirmation. Composer drafts are kept
separately while switching between conversations.

## General and Coder modes

Run `:MedousaMode` or press `<leader>mm` to change the active mode for the
same daemon conversation used by Medousa and VS Code. Coder can be selected
before a project exists. Its setup phase has no file or shell authority.

On the first Coder send, Neovim keeps the draft intact and asks you to choose
a ready Forge project, create a blank project, or let Medousa choose or create
one from that message. Selecting or creating directly continues the original
send after binding. The Medousa option records the picker action as structured
principal authorization while preserving the exact human prompt in history;
full coding tools activate on the following bound turn.

`:MedousaProject` or `<leader>mp` opens the same project picker at any time.
Detaching leaves Coder active in its restricted setup phase. When the governed
worktree is visible on the Neovim host, the plugin offers to change directory;
remote workshop paths remain daemon-owned and are never treated as local paths.
Coder executes in a private attempt worktree. Runtime refreshes expose that same
path through the existing project binding, and interrupted turns preserve it—
including unfinished edits—for the next turn without mutating the staging
worktree.
Separate conversations may run agents against the same project concurrently;
each lease owns a different worktree and branch.

Mode changes proposed by Medousa appear as a Switch/Not now picker after the
turn. The workshop's configured expiry and auto-accept policy remains
authoritative.

## Applying code safely

Medousa does not silently edit the buffer. Ask for a fenced code block, press
`A` or run `:MedousaApply`, and choose a block when there is more than one.
Selection requests target the captured range; other requests let you choose
between insertion at the captured cursor and whole-buffer replacement.

Every change opens as a unified diff first. From the preview, `a` applies the
change as one undoable edit, `y` copies the proposed code, and `q` cancels.
Application is refused when the source buffer changed after the request or
while the preview was open.

The plugin sends a bounded selection or cursor-centered buffer excerpt plus
diagnostics to the daemon. The daemon remains authoritative for sessions,
identity, tools, and remote workshops.
The captured context is attached as turn metadata, so room history shows only
the prompt you wrote.

## Optional statusline

`require("medousa").statusline()` returns a compact connection state such as
`Medousa:on`, `Medousa:think`, or `Medousa:recover`. Statusline plugins can call
it directly; the plugin does not change the user's statusline automatically.

To run the local smoke check:

```bash
NVIM_LOG_FILE=/tmp/medousa-nvim.log nvim --headless -n -i NONE -u NONE \
  --cmd 'set rtp+=integrations/neovim' \
  -l integrations/neovim/tests/smoke.lua

NVIM_LOG_FILE=/tmp/medousa-nvim.log nvim --headless -n -i NONE -u NONE \
  --cmd 'set rtp+=integrations/neovim' \
  -l integrations/neovim/tests/ui_smoke.lua
```
