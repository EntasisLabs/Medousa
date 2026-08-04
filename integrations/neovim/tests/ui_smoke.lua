local Client = require("medousa.client")
local approved_budget = false
local bound_project = false
local setup_authorized = false
local prefer_agent_setup = false

local fake = {
  ensure_session = function(_, callback)
    callback("session-one", { session_id = "session-one", turns = {} }, nil)
  end,
  turn = function(_, _, _, _, callbacks, options)
    setup_authorized = options and options.code_project_setup_authorized == true
    local answer = "Hello from Medousa.\n```lua\nlocal polished = true\n```"
    callbacks.on_event({
      content_delta = vim.NIL,
      final_text = vim.NIL,
      operator_message = vim.NIL,
      tool_name = vim.NIL,
      tool_status = vim.NIL,
      budget_request_id = vim.NIL,
      permission_request_id = vim.NIL,
    })
    callbacks.on_event({ budget_request_id = "budget-one", requested_rounds = 2 })
    callbacks.on_event({ content_delta = "Hello from " })
    callbacks.on_event({ final_text = answer })
    callbacks.on_done({ terminal = true, final_text = answer })
  end,
  sessions = function(_, _, callback)
    callback({ { session_id = "session-one", display_name = "Compiler work" } }, nil)
  end,
  agent_mode = function(_, session_id, callback)
    callback({ session_id = session_id, effective_mode = "coder" }, nil)
  end,
  code_binding = function(_, session_id, callback)
    callback({ session_id = session_id }, nil)
  end,
  mode_proposals = function(_, _, callback) callback({ proposals = {} }, nil) end,
  forge_items = function(_, callback)
    callback({ {
      id = "work-one",
      title = "Compiler",
      brief = "Improve the compiler",
      state = "ready",
      environment = { worktree = "/not-mounted-in-smoke" },
    } }, nil)
  end,
  set_code_binding = function(_, session_id, work_id, callback)
    bound_project = session_id == "session-one" and work_id == "work-one"
    callback({ session_id = session_id, work_id = work_id }, nil)
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
vim.ui.select = function(items, _, callback)
  if prefer_agent_setup then
    for _, item in ipairs(items) do
      if type(item) == "table" and item.action == "agent" then
        callback(item)
        return
      end
    end
  end
  callback(items[1])
end

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
assert(bound_project)
assert(not setup_authorized)
prefer_agent_setup = true
medousa.send("Create the right project for this compiler work")
assert(setup_authorized)
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
