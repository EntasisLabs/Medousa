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

## Use the coding room

Default normal-mode mappings are:

- `<leader>mc` toggles the floating coding room.
- `<leader>ma` opens the prompt.
- `<leader>me` asks about the visual selection, or pre-fills a buffer question.
- `<leader>mf` asks about current diagnostics, or pre-fills a buffer review.

Inside the room, `a` focuses the prompt, `q` closes it, `c` cancels an active
response, and `A` opens the explicit code-application action. The commands
`MedousaToggle`, `MedousaAsk`, `MedousaExplain`, `MedousaFix`, `MedousaApply`,
`MedousaNew`, `MedousaCancel`, and `MedousaStatus` provide the same paths for
custom mappings.

Streaming replies show concise thinking/tool/recovery status and reconnect by
event sequence when the connection is interrupted. Closing the window does
not discard the daemon conversation.

## Applying code safely

Medousa does not silently edit the buffer. Ask for a fenced code block, press
`A` or run `:MedousaApply`, choose a block when there is more than one, and
confirm whether it should replace the captured selection or insert at the
cursor. The plugin sends bounded editor context to the daemon, while the
daemon remains authoritative for sessions, identity, tools, and remote
workshops.

To run the local smoke check:

```bash
nvim --headless -n -i NONE -u NONE \
  --cmd 'set rtp+=integrations/neovim' \
  -l integrations/neovim/tests/smoke.lua
```
