local Client = require("medousa.client")
local approved_budget = false

local fake = {
  ensure_session = function(_, callback)
    callback("session-one", { session_id = "session-one", turns = {} }, nil)
  end,
  turn = function(_, _, _, _, callbacks)
    local answer = "Hello from Medousa.\n```lua\nlocal polished = true\n```"
    callbacks.on_event({ budget_request_id = "budget-one", requested_rounds = 2 })
    callbacks.on_event({ content_delta = "Hello from " })
    callbacks.on_event({ final_text = answer })
    callbacks.on_done({ terminal = true, final_text = answer })
  end,
  sessions = function(_, _, callback)
    callback({ { session_id = "session-one", display_name = "Compiler work" } }, nil)
  end,
  approve_budget = function(_, request_id, rounds, callback)
    approved_budget = request_id == "budget-one" and rounds == 2
    callback({}, nil)
  end,
  health = function(_, callback) callback({}, nil) end,
}

Client.new = function() return fake end
package.loaded["medousa"] = nil
local medousa = require("medousa")
medousa.setup({ keymaps = {} })
local original_select = vim.ui.select
vim.ui.select = function(items, _, callback) callback(items[1]) end

local source = vim.api.nvim_get_current_buf()
vim.api.nvim_buf_set_lines(source, 0, -1, false, { "local answer = 42" })
vim.bo[source].filetype = "lua"
medousa.send("Explain this buffer")

local transcript
local inspected = {}
for _, buffer in ipairs(vim.api.nvim_list_bufs()) do
  if vim.api.nvim_buf_is_valid(buffer) and vim.api.nvim_buf_is_loaded(buffer) then
    local lines = vim.api.nvim_buf_get_lines(buffer, 0, -1, false)
    local text = table.concat(lines, "\n")
    table.insert(inspected, text)
    if text:find("Hello from Medousa", 1, true) then transcript = text end
  end
end

assert(transcript and transcript:find("You ·", 1, true), vim.inspect(inspected))
assert(medousa.statusline() == "Medousa:on")
assert(approved_budget)
medousa.toggle()

medousa.ask("unfinished\nthought")
medousa.toggle()
medousa.toggle()
local restored_draft = false
for _, buffer in ipairs(vim.api.nvim_list_bufs()) do
  if vim.api.nvim_buf_is_valid(buffer) and vim.api.nvim_buf_is_loaded(buffer) then
    local text = table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), "\n")
    if text == "unfinished\nthought" then restored_draft = true end
  end
end
assert(restored_draft)
medousa.toggle()
vim.ui.select = original_select

print("neovim ui smoke: ok")
