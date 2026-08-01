local stream = require("medousa.stream")
local context = require("medousa.context")
local edit = require("medousa.edit")
local Client = require("medousa.client")

local events = {}
local parse = stream.new_parser(function(value, name)
  table.insert(events, { event = value, name = name })
end)

parse("event: content\ndata: {\"content_delta\":\"hi\"}\n\n")
assert(#events == 1)
assert(events[1].name == "content")
assert(events[1].event.content_delta == "hi")

local blocks = stream.extract_code_blocks("x\n```rust\nlet x = 1;\n```\n")
assert(#blocks == 1, vim.inspect(blocks))
assert(blocks[1].language == "rust")
assert(blocks[1].text == "let x = 1;")
assert(blocks[1].start_line == 2 and blocks[1].end_line == 4)
local plain = stream.extract_code_blocks("```\nplain\n```   ")
assert(#plain == 1 and plain[1].language == "text" and plain[1].text == "plain")

local medousa = require("medousa")
medousa.setup({ keymaps = {} })
assert(vim.fn.exists(":MedousaToggle") == 2)
assert(vim.fn.exists(":MedousaApply") == 2)
assert(vim.fn.exists(":MedousaSessions") == 2)
assert(vim.fn.exists(":MedousaRename") == 2)
assert(vim.fn.exists(":MedousaAttention") == 2)

local buffer = vim.api.nvim_create_buf(false, true)
vim.api.nvim_set_current_buf(buffer)
vim.api.nvim_buf_set_lines(buffer, 0, -1, false, { "one", "two", "three" })
vim.bo[buffer].filetype = "lua"
local captured = context.from_range(buffer, 2, 2)
assert(captured.selection.text == "two")
assert(context.supplement(captured):find("selection%-lines: 2%-2"))
assert(context.strip_supplement("hello\n\n" .. context.supplement(captured)) == "hello")

local prepared = assert(edit.prepare({ language = "lua", text = "replaced" }, captured, "selection"))
assert(prepared.diff:find("%-two"))
assert(prepared.diff:find("%+replaced"))
assert(edit.apply(prepared))
assert(vim.deep_equal(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), { "one", "replaced", "three" }))

local insert_context = context.current({ buffer = buffer, cursor = { 2, 0 } })
local insertion = assert(edit.prepare({ language = "lua", text = "inserted" }, insert_context, "insert"))
assert(vim.deep_equal(insertion.after, { "one", "replaced", "inserted", "three" }))
local replacement = assert(edit.prepare({ language = "lua", text = "whole" }, insert_context, "buffer"))
assert(vim.deep_equal(replacement.after, { "whole" }))

local stale_context = context.from_range(buffer, 2, 2)
local stale = assert(edit.prepare({ language = "lua", text = "stale" }, stale_context, "selection"))
vim.api.nvim_buf_set_lines(buffer, 0, 1, false, { "changed" })
local applied, stale_error = edit.apply(stale)
assert(not applied)
assert(stale_error:find("changed"))

local client = Client.new({ endpoint = "http://127.0.0.1:7419" })
local request
client.request = function(_, method, path, body, callback)
  request = { method = method, path = path, body = body }
  callback({ sessions = {} }, nil)
end
client:sessions(12, function(items) assert(#items == 0) end)
assert(request.method == "GET" and request.path == "/v1/sessions?limit=12")
client:rename_session("session/one", "Compiler work", function() end)
assert(request.method == "PUT" and request.path == "/v1/sessions/session%2Fone/name")
assert(request.body.display_name == "Compiler work")
client:approve_budget("budget/one", 3, function() end)
assert(request.path == "/v1/turns/budget-requests/budget%2Fone/approve")
assert(request.body.extra_rounds == 3 and request.body.resolved_by == "neovim")
client:resolve_permission("permission/one", false, function() end)
assert(request.path == "/v1/agents/permission-requests/permission%2Fone/deny")

print("neovim stream smoke: ok")
