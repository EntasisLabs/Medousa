local util = require("medousa.util")

local M = {}

local function copy_lines(lines)
  local copy = {}
  for index, line in ipairs(lines) do copy[index] = line end
  return copy
end

local function replace_range(lines, first, last, replacement)
  local result = {}
  for index = 1, first - 1 do table.insert(result, lines[index]) end
  vim.list_extend(result, replacement)
  for index = last + 1, #lines do table.insert(result, lines[index]) end
  return result
end

function M.prepare(block, value, target)
  if not value or not value.buffer or not vim.api.nvim_buf_is_valid(value.buffer) then
    return nil, "The original buffer is no longer available."
  end
  if value.changedtick ~= vim.api.nvim_buf_get_changedtick(value.buffer) then
    return nil, "The buffer changed since this answer was requested; ask again before applying it."
  end

  local before = vim.api.nvim_buf_get_lines(value.buffer, 0, -1, false)
  local replacement = vim.split(block.text, "\n", { plain = true })
  local prepared = {
    buffer = value.buffer,
    changedtick = value.changedtick,
    before = copy_lines(before),
    replacement = replacement,
    language = block.language,
    target = target,
  }

  if target == "selection" then
    prepared.first = value.selection_start - 1
    prepared.last = value.selection_end
    prepared.after = replace_range(before, value.selection_start, value.selection_end, replacement)
    prepared.label = "replace lines " .. value.selection_start .. "–" .. value.selection_end
  elseif target == "buffer" then
    prepared.first = 0
    prepared.last = #before
    prepared.after = replacement
    prepared.label = "replace buffer"
  else
    local line = value.cursor and value.cursor.line or #before
    prepared.first = line
    prepared.last = line
    prepared.after = replace_range(before, line + 1, line, replacement)
    prepared.label = "insert after line " .. line
  end

  local before_text = table.concat(prepared.before, "\n") .. "\n"
  local after_text = table.concat(prepared.after, "\n") .. "\n"
  prepared.diff = vim.diff(before_text, after_text, {
    result_type = "unified",
    ctxlen = 4,
    algorithm = "histogram",
  })
  return prepared, nil
end

function M.apply(prepared)
  if not vim.api.nvim_buf_is_valid(prepared.buffer) then
    return nil, "The original buffer is no longer available."
  end
  if prepared.changedtick ~= vim.api.nvim_buf_get_changedtick(prepared.buffer) then
    return nil, "The buffer changed while the preview was open. Nothing was applied."
  end
  vim.api.nvim_buf_set_lines(prepared.buffer, prepared.first, prepared.last, false, prepared.replacement)
  return true, nil
end

function M.choose_target(value, callback)
  if value.selection then
    callback("selection")
    return
  end
  local targets = {
    { id = "insert", label = "Insert after captured cursor", detail = "Keep the existing buffer" },
    { id = "buffer", label = "Replace the entire buffer", detail = "Review the complete diff first" },
  }
  vim.ui.select(targets, {
    prompt = "Where should this code go?",
    format_item = function(item) return item.label .. " · " .. item.detail end,
  }, function(choice)
    if choice then callback(choice.id) end
  end)
end

function M.preview(prepared, callbacks, options)
  options = options or {}
  local width = math.max(56, math.floor(vim.o.columns * 0.78))
  local height = math.max(14, math.floor(vim.o.lines * 0.72))
  local row = math.max(1, math.floor((vim.o.lines - height) / 2) - 1)
  local col = math.max(1, math.floor((vim.o.columns - width) / 2))
  local buffer = vim.api.nvim_create_buf(false, true)
  local diff_lines = vim.split(prepared.diff or "", "\n", { plain = true })
  if #diff_lines == 0 or (#diff_lines == 1 and diff_lines[1] == "") then
    diff_lines = { "No changes to preview." }
  end
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, diff_lines)
  vim.bo[buffer].filetype = "diff"
  vim.bo[buffer].modifiable = false
  vim.bo[buffer].bufhidden = "wipe"
  local window = vim.api.nvim_open_win(buffer, true, {
    relative = "editor",
    row = row,
    col = col,
    width = width,
    height = height,
    style = "minimal",
    border = options.border or "rounded",
    title = " Medousa · " .. prepared.label .. " ",
    title_pos = "center",
    footer = " a apply · y copy · q cancel ",
    footer_pos = "center",
  })
  vim.wo[window].wrap = false
  vim.wo[window].cursorline = true

  local function close()
    if vim.api.nvim_win_is_valid(window) then vim.api.nvim_win_close(window, true) end
  end
  vim.keymap.set("n", "q", close, { buffer = buffer, silent = true, desc = "Cancel Medousa edit" })
  vim.keymap.set("n", "<Esc>", close, { buffer = buffer, silent = true, desc = "Cancel Medousa edit" })
  vim.keymap.set("n", "y", function()
    util.copy(table.concat(prepared.replacement, "\n"))
    if callbacks.on_copy then callbacks.on_copy() end
  end, { buffer = buffer, silent = true, desc = "Copy Medousa code" })
  vim.keymap.set("n", "a", function()
    local ok, err = M.apply(prepared)
    if not ok then
      if callbacks.on_error then callbacks.on_error(err) end
      return
    end
    close()
    if callbacks.on_apply then callbacks.on_apply(prepared) end
  end, { buffer = buffer, silent = true, desc = "Apply Medousa edit" })
  return { buffer = buffer, window = window }
end

function M.block_label(block)
  return block.language .. " · " .. util.first_line(block.text)
end

return M
