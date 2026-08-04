# Medousa for Neovim

A keyboard-first Medousa coding room for Neovim 0.10+. It uses the same
workshop, sessions, identity, and streaming runtime as the Medousa app while
staying transient and native to editing.

## Setup

```lua
{
  dir = "/path/to/Medousa/integrations/neovim",
  config = function()
    require("medousa").setup()
  end,
}
```

The default workshop is `http://127.0.0.1:7419`. `curl` is required. For a
paired workshop:

```lua
require("medousa").setup({
  endpoint = "https://your-workshop.example",
  token = vim.env.MEDOUSA_TOKEN,
  width = 0.72,
  height = 0.72,
  composer_height = 5,
  border = "rounded",
  restore_focus = true,
  keymaps = {
    toggle = "<leader>mc",
    ask = "<leader>ma",
    explain = "<leader>me",
    fix = "<leader>mf",
    operator = "<leader>mo",
    sessions = "<leader>ms",
    mode = "<leader>mm",
    project = "<leader>mp",
  },
})
```

Set any keymap to `false` to leave it unbound. Telescope is used for
conversation selection when present and is otherwise optional.

## Editing flow

- Open with `<leader>mc`, then compose normally and use Ctrl-S to send.
- Use visual `<leader>ma`, ranged `:MedousaAsk`, or `<leader>mo{motion}` to
  anchor a question to code.
- Use `<leader>me` for explanation and `<leader>mf` for diagnostics-aware fixes.
- Press `A` on a settled answer—or directly inside a code fence—to inspect a
  unified diff. Apply with `a`, copy with `y`, or cancel with `q`.
- Press `<leader>ms` to continue, rename, or delete daemon-owned conversations.
- Press `<leader>mm` to switch General/Coder and `<leader>mp` to choose,
  create, or detach a daemon-governed project. An unbound Coder send opens the
  same picker without consuming the draft.
- Resolve budget and tool-permission prompts in place; `:MedousaAttention`
  reopens a decision that was dismissed.

Run `:help medousa` for the complete command and in-room key reference.

## Tests

```bash
NVIM_LOG_FILE=/tmp/medousa-nvim.log nvim --headless -n -i NONE -u NONE \
  --cmd 'set rtp+=integrations/neovim' \
  -l integrations/neovim/tests/smoke.lua

NVIM_LOG_FILE=/tmp/medousa-nvim.log nvim --headless -n -i NONE -u NONE \
  --cmd 'set rtp+=integrations/neovim' \
  -l integrations/neovim/tests/ui_smoke.lua
```
