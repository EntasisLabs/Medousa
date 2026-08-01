local M = {}

local function visual_range()
  local mode = vim.fn.mode()
  if not (mode == "v" or mode == "V" or mode == "\22") then
    return nil
  end
  local start = vim.fn.getpos("<")[2]
  local finish = vim.fn.getpos(">")[2]
  if start > finish then
    start, finish = finish, start
  end
  return start, finish
end

function M.current()
  local buffer = vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(buffer)
  local lines = vim.api.nvim_buf_get_lines(buffer, 0, -1, false)
  local start, finish = visual_range()
  local selection
  if start and finish then
    selection = table.concat(vim.list_slice(lines, start, finish), "\n")
  end

  local diagnostics = {}
  for _, item in ipairs(vim.diagnostic.get(buffer)) do
    table.insert(diagnostics, {
      message = item.message,
      severity = item.severity == vim.diagnostic.severity.ERROR and "error"
        or item.severity == vim.diagnostic.severity.WARN and "warning"
        or "info",
      range = {
        start = { line = item.lnum, character = item.col },
        ["end"] = { line = item.end_lnum or item.lnum, character = item.end_col or item.col },
      },
    })
  end

  return {
    buffer = buffer,
    changedtick = vim.api.nvim_buf_get_changedtick(buffer),
    file = name ~= "" and name or nil,
    workspace = vim.fn.getcwd(),
    language = vim.bo.filetype ~= "" and vim.bo.filetype or nil,
    selection = selection and { text = selection, start = { line = start - 1, character = 0 }, ["end"] = { line = finish, character = 0 } } or nil,
    diagnostics = diagnostics,
    selection_start = start,
    selection_end = finish,
  }
end

function M.supplement(value)
  local lines = { "<medousa-context>", "surface: neovim" }
  if value.workspace then table.insert(lines, "workspace: " .. value.workspace) end
  if value.file then table.insert(lines, "file: " .. value.file) end
  if value.language then table.insert(lines, "language: " .. value.language) end
  if value.selection then
    table.insert(lines, "selection:")
    table.insert(lines, "```")
    table.insert(lines, value.selection.text:sub(1, 12000))
    table.insert(lines, "```")
  end
  if #value.diagnostics > 0 then
    table.insert(lines, "diagnostics:")
    for index, item in ipairs(value.diagnostics) do
      if index > 100 then break end
      table.insert(lines, "- " .. item.message)
    end
  end
  table.insert(lines, "</medousa-context>")
  return table.concat(lines, "\n")
end

function M.describe(value)
  local parts = {}
  if value.file then table.insert(parts, vim.fn.fnamemodify(value.file, ":t")) end
  if value.selection then table.insert(parts, "selection") end
  if #value.diagnostics > 0 then table.insert(parts, #value.diagnostics .. " diagnostics") end
  return #parts > 0 and table.concat(parts, " · ") or "current workspace"
end

return M
