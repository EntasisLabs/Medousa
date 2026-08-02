local M = {}

local MAX_CONTEXT_CHARS = 12000
local EXCERPT_RADIUS = 80

local function clamp(value, minimum, maximum)
  return math.max(minimum, math.min(maximum, value))
end

local function range_lines(buffer, first, last)
  local count = vim.api.nvim_buf_line_count(buffer)
  first = clamp(first or 1, 1, count)
  last = clamp(last or first, first, count)
  return vim.api.nvim_buf_get_lines(buffer, first - 1, last, false), first, last
end

local function visual_range()
  local mode = vim.fn.mode(1)
  if not (mode:sub(1, 1) == "v" or mode:sub(1, 1) == "V" or mode:sub(1, 1) == "\22") then
    return nil
  end
  local first = vim.fn.line("v")
  local last = vim.fn.line(".")
  if first <= 0 or last <= 0 then return nil end
  if first > last then first, last = last, first end
  return first, last
end

local function diagnostic_context(buffer)
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
  return diagnostics
end

function M.current(options)
  options = options or {}
  local buffer = options.buffer or vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(buffer)
  local cursor = options.cursor or vim.api.nvim_win_get_cursor(0)
  local first, last = options.line1, options.line2
  if not first then first, last = visual_range() end

  local selection
  if first and last then
    local lines
    lines, first, last = range_lines(buffer, first, last)
    selection = table.concat(lines, "\n"):sub(1, MAX_CONTEXT_CHARS)
  end

  local excerpt_first = clamp(cursor[1] - EXCERPT_RADIUS, 1, vim.api.nvim_buf_line_count(buffer))
  local excerpt_last = clamp(cursor[1] + EXCERPT_RADIUS, excerpt_first, vim.api.nvim_buf_line_count(buffer))
  local excerpt_lines = range_lines(buffer, excerpt_first, excerpt_last)

  return {
    buffer = buffer,
    changedtick = vim.api.nvim_buf_get_changedtick(buffer),
    cursor = { line = cursor[1], character = cursor[2] },
    file = name ~= "" and name or nil,
    workspace = vim.fn.getcwd(),
    language = vim.bo[buffer].filetype ~= "" and vim.bo[buffer].filetype or nil,
    selection = selection and {
      text = selection,
      start = { line = first - 1, character = 0 },
      ["end"] = { line = last, character = 0 },
    } or nil,
    excerpt = selection and nil or {
      text = table.concat(excerpt_lines, "\n"):sub(1, MAX_CONTEXT_CHARS),
      start_line = excerpt_first,
      end_line = excerpt_last,
    },
    diagnostics = diagnostic_context(buffer),
    selection_start = first,
    selection_end = last,
  }
end

function M.from_range(buffer, first, last)
  return M.current({ buffer = buffer, line1 = first, line2 = last })
end

function M.from_operator(buffer)
  local first = vim.fn.getpos("'[")[2]
  local last = vim.fn.getpos("']")[2]
  if first > last then first, last = last, first end
  return M.from_range(buffer, first, last)
end

function M.supplement(value)
  local lines = { "<medousa-context>", "surface: neovim" }
  if value.workspace then table.insert(lines, "workspace: " .. value.workspace) end
  if value.file then table.insert(lines, "file: " .. value.file) end
  if value.language then table.insert(lines, "language: " .. value.language) end
  if value.cursor then table.insert(lines, "cursor: " .. value.cursor.line .. ":" .. (value.cursor.character + 1)) end
  if value.selection then
    table.insert(lines, "selection-lines: " .. value.selection_start .. "-" .. value.selection_end)
    table.insert(lines, "selection:")
    table.insert(lines, "```")
    table.insert(lines, value.selection.text)
    table.insert(lines, "```")
  elseif value.excerpt then
    table.insert(lines, "buffer-excerpt-lines: " .. value.excerpt.start_line .. "-" .. value.excerpt.end_line)
    table.insert(lines, "buffer-excerpt:")
    table.insert(lines, "```")
    table.insert(lines, value.excerpt.text)
    table.insert(lines, "```")
  end
  if #value.diagnostics > 0 then
    table.insert(lines, "diagnostics:")
    for index, item in ipairs(value.diagnostics) do
      if index > 100 then break end
      table.insert(lines, string.format(
        "- %s at %d:%d: %s",
        item.severity,
        item.range.start.line + 1,
        item.range.start.character + 1,
        item.message
      ))
    end
  end
  table.insert(lines, "</medousa-context>")
  return table.concat(lines, "\n")
end

function M.describe(value)
  local parts = {}
  if value.file then table.insert(parts, vim.fn.fnamemodify(value.file, ":t")) end
  if value.selection then table.insert(parts, "lines " .. value.selection_start .. "–" .. value.selection_end) end
  if #value.diagnostics > 0 then table.insert(parts, #value.diagnostics .. " diagnostics") end
  return #parts > 0 and table.concat(parts, " · ") or "current workspace"
end

function M.strip_supplement(content)
  local stripped = (type(content) == "string" and content or ""):gsub("\n*<medousa%-context>.-</medousa%-context>%s*$", "")
  return stripped
end

return M
