local stream = require("medousa.stream")

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

local medousa = require("medousa")
medousa.setup({ keymaps = {} })
assert(vim.fn.exists(":MedousaToggle") == 2)
assert(vim.fn.exists(":MedousaApply") == 2)

print("neovim stream smoke: ok")
