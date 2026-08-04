local Client = require("medousa.client")
local context = require("medousa.context")
local edit = require("medousa.edit")
local picker = require("medousa.picker")
local stream = require("medousa.stream")
local util = require("medousa.util")

local M = {}
local namespace = vim.api.nvim_create_namespace("medousa-neovim")
local normalize_session
local refresh_runtime_state
local check_mode_proposals

local state = {
  client = nil,
  configured = false,
  session_id = nil,
  session_name = nil,
  agent_mode = "general",
  mode_label = "General",
  binding_work_id = nil,
  project = nil,
  connection = "idle",
  chat_buf = nil,
  chat_win = nil,
  prompt_buf = nil,
  prompt_win = nil,
  origin_win = nil,
  room_context = nil,
  pending_context = nil,
  messages = {},
  answer = nil,
  busy = false,
  current_status = nil,
  tools = {},
  tool_order = {},
  show_tools = false,
  handled_requests = {},
  handled_mode_proposals = {},
  pending_attention = nil,
  last_context = nil,
  last_answer = "",
  last_prompt = nil,
  history_signature = "",
  drafts = {},
  draft_contexts = {},
  connecting = false,
  connection_waiters = {},
  resize_autocmd = nil,
  closing = false,
}

local defaults = {
  endpoint = "http://127.0.0.1:7419",
  token = nil,
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
}

local function valid_buffer(buffer)
  return buffer and vim.api.nvim_buf_is_valid(buffer)
end

local function valid_window(window)
  return window and vim.api.nvim_win_is_valid(window)
end

local function is_room_window(window)
  return window == state.chat_win or window == state.prompt_win
end

local function session_key()
  return state.session_id or "__new"
end

local function notify(message, level)
  vim.schedule(function()
    vim.notify("Medousa: " .. message, level or vim.log.levels.INFO)
  end)
end

local function set_lines(buffer, lines)
  if not valid_buffer(buffer) then return end
  vim.bo[buffer].modifiable = true
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, lines)
  vim.bo[buffer].modifiable = false
end

local function prompt_text()
  if not valid_buffer(state.prompt_buf) then return "" end
  return util.trim(table.concat(vim.api.nvim_buf_get_lines(state.prompt_buf, 0, -1, false), "\n"))
end

local function text_value(value)
  return type(value) == "string" and value or ""
end

local function nonempty_text(value)
  local text = text_value(value)
  return text ~= "" and text or nil
end

local function save_draft()
  if not valid_buffer(state.prompt_buf) then return end
  local key = session_key()
  local draft = prompt_text()
  state.drafts[key] = draft
  state.draft_contexts[key] = draft ~= "" and (state.pending_context or state.room_context) or nil
  if draft == "" then state.pending_context = nil end
end

local function set_prompt(value)
  if not valid_buffer(state.prompt_buf) then return end
  local lines = vim.split(text_value(value), "\n", { plain = true })
  if #lines == 0 then lines = { "" } end
  vim.bo[state.prompt_buf].modifiable = true
  vim.api.nvim_buf_set_lines(state.prompt_buf, 0, -1, false, lines)
end

local function focus_prompt()
  if valid_window(state.prompt_win) then
    vim.api.nvim_set_current_win(state.prompt_win)
    vim.cmd("startinsert")
  end
end

local function room_layout()
  local width = math.min(vim.o.columns - 4, math.max(44, math.floor(vim.o.columns * defaults.width)))
  local total_height = math.min(vim.o.lines - 6, math.max(18, math.floor(vim.o.lines * defaults.height)))
  local composer_height = math.min(defaults.composer_height, math.max(3, total_height - 10))
  local chat_height = total_height - composer_height - 2
  local row = math.max(1, math.floor((vim.o.lines - total_height) / 2) - 1)
  local col = math.max(1, math.floor((vim.o.columns - width) / 2))
  return {
    width = width,
    chat_height = chat_height,
    composer_height = composer_height,
    row = row,
    col = col,
    prompt_row = row + chat_height + 2,
  }
end

local function connection_label()
  if state.busy then return "thinking" end
  if state.connection == "recovering" then return "recovering" end
  if state.connection == "connected" then return "connected" end
  if state.connection == "unauthorized" then return "authorization required" end
  if state.connection == "unavailable" then return "unavailable" end
  return "connecting"
end

local function failed_connection(err)
  local message = tostring(err or "")
  return (message:find("401", 1, true) or message:find("403", 1, true)) and "unauthorized" or "unavailable"
end

local function update_window_titles()
  if valid_window(state.chat_win) then
    local name = state.session_name or "Coding room"
    local mode = state.mode_label
    if state.agent_mode == "coder" then
      mode = state.project and ("Coder · " .. state.project.title) or "Coder setup"
    end
    vim.api.nvim_win_set_config(state.chat_win, {
      title = " Medousa · " .. name .. " · " .. mode .. " · " .. connection_label() .. " ",
      title_pos = "center",
    })
  end
end

local function message_lines()
  local lines, marks = {}, {}
  if #state.messages == 0 and not state.answer then
    local value = state.room_context
    table.insert(lines, "")
    table.insert(lines, "Medousa is ready in " .. (value and context.describe(value) or "your workspace") .. ".")
    table.insert(lines, "")
    table.insert(lines, "Ask about the code, explain a range, fix diagnostics, or describe the change you want.")
    table.insert(lines, "")
    table.insert(lines, "  a  focus composer    s  conversations    m  mode    p  project")
    table.insert(lines, "  A  preview last edit c  stop response    n  new      q  return to code")
  end

  for _, message in ipairs(state.messages) do
    if message.role == "user" then
      table.insert(lines, "You · " .. (nonempty_text(message.context_label) or "current code"))
      table.insert(marks, { line = #lines - 1, group = "Title" })
      vim.list_extend(lines, vim.split(text_value(message.content), "\n", { plain = true }))
      table.insert(lines, "")
    elseif message.role == "assistant" then
      table.insert(lines, "Medousa")
      table.insert(marks, { line = #lines - 1, group = "Title" })
      vim.list_extend(lines, vim.split(text_value(message.content), "\n", { plain = true }))
      table.insert(lines, "")
    elseif message.role == "error" then
      table.insert(lines, "Error · " .. text_value(message.content))
      table.insert(marks, { line = #lines - 1, group = "DiagnosticError" })
      table.insert(lines, "")
    elseif message.role == "attention" then
      table.insert(lines, "Attention · " .. text_value(message.content))
      table.insert(marks, { line = #lines - 1, group = "DiagnosticWarn" })
      table.insert(lines, "")
    end
  end

  if state.answer ~= nil and state.answer ~= vim.NIL then
    table.insert(lines, "Medousa · " .. (nonempty_text(state.current_status) or "thinking"))
    table.insert(marks, { line = #lines - 1, group = "Title" })
    local answer = text_value(state.answer)
    if answer ~= "" then vim.list_extend(lines, vim.split(answer, "\n", { plain = true })) end
    table.insert(lines, "")
  elseif nonempty_text(state.current_status) then
    table.insert(lines, "· " .. text_value(state.current_status))
    table.insert(marks, { line = #lines - 1, group = "Comment" })
  end

  if #state.tool_order > 0 then
    if state.show_tools then
      table.insert(lines, "")
      table.insert(lines, "Tools")
      table.insert(marks, { line = #lines - 1, group = "Title" })
      for _, key in ipairs(state.tool_order) do
        local tool = state.tools[key]
        table.insert(lines, "  · " .. tool.name .. " · " .. tool.status)
      end
    else
      table.insert(lines, "· " .. #state.tool_order .. " tool" .. (#state.tool_order == 1 and "" or "s") .. " · Tab to show")
      table.insert(marks, { line = #lines - 1, group = "Comment" })
    end
  end
  return lines, marks
end

local function render()
  if not valid_buffer(state.chat_buf) then return end
  local old_count = vim.api.nvim_buf_line_count(state.chat_buf)
  local pinned = not valid_window(state.chat_win)
  if valid_window(state.chat_win) then
    pinned = vim.api.nvim_win_get_cursor(state.chat_win)[1] >= old_count - 2
  end
  local lines, marks = message_lines()
  if #lines == 0 then lines = { "" } end
  set_lines(state.chat_buf, lines)
  vim.api.nvim_buf_clear_namespace(state.chat_buf, namespace, 0, -1)
  for _, mark in ipairs(marks) do
    vim.api.nvim_buf_add_highlight(state.chat_buf, namespace, mark.group, mark.line, 0, -1)
  end
  if pinned and valid_window(state.chat_win) then
    vim.api.nvim_win_set_cursor(state.chat_win, { vim.api.nvim_buf_line_count(state.chat_buf), 0 })
  end
  update_window_titles()
  vim.cmd("redrawstatus")
end

local function resize_room()
  if not (valid_window(state.chat_win) and valid_window(state.prompt_win)) then return end
  local layout = room_layout()
  vim.api.nvim_win_set_config(state.chat_win, {
    row = layout.row,
    col = layout.col,
    width = layout.width,
    height = layout.chat_height,
  })
  vim.api.nvim_win_set_config(state.prompt_win, {
    row = layout.prompt_row,
    col = layout.col,
    width = layout.width,
    height = layout.composer_height,
  })
end

local function close_room()
  if state.closing then return end
  state.closing = true
  save_draft()
  local return_window = state.origin_win
  if valid_window(state.prompt_win) then vim.api.nvim_win_close(state.prompt_win, true) end
  if valid_window(state.chat_win) then vim.api.nvim_win_close(state.chat_win, true) end
  if valid_buffer(state.prompt_buf) then pcall(vim.api.nvim_buf_delete, state.prompt_buf, { force = true }) end
  if valid_buffer(state.chat_buf) then pcall(vim.api.nvim_buf_delete, state.chat_buf, { force = true }) end
  if state.resize_autocmd then pcall(vim.api.nvim_del_autocmd, state.resize_autocmd) end
  state.resize_autocmd = nil
  state.chat_buf, state.chat_win, state.prompt_buf, state.prompt_win = nil, nil, nil, nil
  if defaults.restore_focus and valid_window(return_window) then vim.api.nvim_set_current_win(return_window) end
  state.closing = false
end

local function capture_current()
  local current = vim.api.nvim_get_current_win()
  if not is_room_window(current) then return context.current() end
  if valid_window(state.origin_win) then
    return vim.api.nvim_win_call(state.origin_win, function() return context.current() end)
  end
  return state.room_context or context.current()
end

local function send_composer()
  local prompt = prompt_text()
  if prompt == "" then
    notify("Write a prompt first.", vim.log.levels.WARN)
    return
  end
  state.drafts[session_key()] = ""
  state.draft_contexts[session_key()] = nil
  set_prompt("")
  M.send(prompt, state.pending_context)
  state.pending_context = nil
end

local function navigate_code_block(direction)
  if not valid_window(state.chat_win) then return end
  vim.api.nvim_set_current_win(state.chat_win)
  vim.fn.search("^```", direction < 0 and "bW" or "W")
end

local function code_block_at_cursor()
  if not (valid_window(state.chat_win) and vim.api.nvim_get_current_win() == state.chat_win) then return nil end
  local cursor_line = vim.api.nvim_win_get_cursor(state.chat_win)[1]
  local lines = vim.api.nvim_buf_get_lines(state.chat_buf, 0, -1, false)
  local opening, language
  for line_number = 1, math.min(cursor_line, #lines) do
    local fence = lines[line_number]:match("^%s*```([%w_+%-]*)%s*$")
    if fence ~= nil then
      if opening then
        if cursor_line <= line_number then
          local body = {}
          for body_line = opening + 1, line_number - 1 do table.insert(body, lines[body_line]) end
          return { language = language, text = table.concat(body, "\n") }
        end
        opening, language = nil, nil
      else
        opening = line_number
        language = fence ~= "" and fence or "text"
      end
    end
  end
  if not opening then return nil end
  for line_number = cursor_line + 1, #lines do
    if lines[line_number]:match("^%s*```[%w_+%-]*%s*$") then
      local body = {}
      for body_line = opening + 1, line_number - 1 do table.insert(body, lines[body_line]) end
      return { language = language, text = table.concat(body, "\n") }
    end
  end
  return nil
end

local function create_room(value)
  if valid_window(state.chat_win) then return end
  local current = vim.api.nvim_get_current_win()
  if not is_room_window(current) then state.origin_win = current end
  state.room_context = value or state.room_context or capture_current()
  local layout = room_layout()

  state.chat_buf = vim.api.nvim_create_buf(false, true)
  state.chat_win = vim.api.nvim_open_win(state.chat_buf, true, {
    relative = "editor",
    row = layout.row,
    col = layout.col,
    width = layout.width,
    height = layout.chat_height,
    style = "minimal",
    border = defaults.border,
    title = " Medousa ",
    title_pos = "center",
    footer = " a ask · m mode · p project · A edit · s sessions · Tab tools · q close ",
    footer_pos = "center",
  })
  vim.bo[state.chat_buf].filetype = "markdown"
  vim.bo[state.chat_buf].bufhidden = "wipe"
  vim.bo[state.chat_buf].modifiable = false
  vim.wo[state.chat_win].wrap = true
  vim.wo[state.chat_win].linebreak = true
  vim.wo[state.chat_win].cursorline = true
  vim.wo[state.chat_win].conceallevel = 2

  state.prompt_buf = vim.api.nvim_create_buf(false, true)
  state.prompt_win = vim.api.nvim_open_win(state.prompt_buf, false, {
    relative = "editor",
    row = layout.prompt_row,
    col = layout.col,
    width = layout.width,
    height = layout.composer_height,
    style = "minimal",
    border = defaults.border,
    title = " Composer · Ctrl-S send · Enter newline ",
    title_pos = "left",
  })
  vim.bo[state.prompt_buf].buftype = "nofile"
  vim.bo[state.prompt_buf].bufhidden = "wipe"
  vim.bo[state.prompt_buf].swapfile = false
  vim.bo[state.prompt_buf].filetype = "markdown"
  vim.wo[state.prompt_win].wrap = true
  vim.wo[state.prompt_win].linebreak = true
  local draft = state.drafts[session_key()] or ""
  set_prompt(draft)
  if draft ~= "" then state.pending_context = state.draft_contexts[session_key()] or state.room_context end

  local map = function(buffer, mode, lhs, action, description)
    vim.keymap.set(mode, lhs, action, { buffer = buffer, silent = true, desc = description })
  end
  map(state.chat_buf, "n", "q", close_room, "Close Medousa")
  map(state.chat_buf, "n", "<Esc>", close_room, "Close Medousa")
  map(state.chat_buf, "n", "a", focus_prompt, "Ask Medousa")
  map(state.chat_buf, "n", "A", M.apply, "Preview Medousa edit")
  map(state.chat_buf, "n", "c", M.cancel, "Cancel Medousa response")
  map(state.chat_buf, "n", "r", M.retry, "Retry last Medousa prompt")
  map(state.chat_buf, "n", "n", M.new_session, "New Medousa conversation")
  map(state.chat_buf, "n", "s", M.sessions, "Medousa conversations")
  map(state.chat_buf, "n", "m", M.select_mode, "Select Medousa mode")
  map(state.chat_buf, "n", "p", M.projects, "Select Medousa project")
  map(state.chat_buf, "n", "R", M.rename_session, "Rename Medousa conversation")
  map(state.chat_buf, "n", "D", M.delete_session, "Delete Medousa conversation")
  map(state.chat_buf, "n", "y", M.copy_last, "Copy last Medousa answer")
  map(state.chat_buf, "n", "Y", M.copy_code, "Copy Medousa code block")
  map(state.chat_buf, "n", "<Tab>", function() state.show_tools = not state.show_tools; render() end, "Toggle Medousa tools")
  map(state.chat_buf, "n", "]c", function() navigate_code_block(1) end, "Next Medousa code block")
  map(state.chat_buf, "n", "[c", function() navigate_code_block(-1) end, "Previous Medousa code block")
  map(state.prompt_buf, "n", "q", close_room, "Close Medousa")
  map(state.prompt_buf, "n", "<C-s>", send_composer, "Send to Medousa")
  map(state.prompt_buf, "n", "<CR>", send_composer, "Send to Medousa")
  map(state.prompt_buf, "i", "<C-s>", function() vim.schedule(send_composer) end, "Send to Medousa")
  map(state.prompt_buf, "i", "<C-c>", function() vim.schedule(M.cancel) end, "Cancel Medousa response")

  state.resize_autocmd = vim.api.nvim_create_autocmd("VimResized", { callback = resize_room })
  for _, buffer in ipairs({ state.chat_buf, state.prompt_buf }) do
    local target_buffer = buffer
    vim.api.nvim_create_autocmd("BufWipeout", {
      buffer = target_buffer,
      once = true,
      callback = function()
        if not state.closing then
          if target_buffer == state.prompt_buf then save_draft() end
          vim.schedule(close_room)
        end
      end,
    })
  end
  render()
end

local function history_signature(history)
  local parts = {}
  for _, turn in ipairs(history.turns or {}) do
    table.insert(parts, tostring(turn.role or "") .. "\0" .. tostring(turn.timestamp or "") .. "\0" .. tostring(turn.content or ""))
  end
  return table.concat(parts, "\1")
end

local function load_history_into_state(history)
  state.messages = {}
  state.last_answer = ""
  for _, turn in ipairs(history.turns or {}) do
    if turn.role == "user" or turn.role == "assistant" then
      local content_value = turn.content
      table.insert(state.messages, { role = turn.role, content = content_value })
      if turn.role == "assistant" then state.last_answer = content_value end
    end
  end
end

local function poll_workshop_history(session_id, attempts)
  if state.session_id ~= session_id or not state.client then return end
  state.client:history(session_id, function(history)
    if state.session_id ~= session_id then return end
    if history and not state.busy then
      local signature = history_signature(history)
      if signature ~= state.history_signature then
        load_history_into_state(history)
        state.history_signature = signature
        state.answer = nil
        state.pending_attention = nil
        state.current_status = nil
        state.connection = "connected"
        render()
      end
    end
    if attempts > 1 then
      vim.defer_fn(function()
        poll_workshop_history(session_id, attempts - 1)
      end, 700)
    end
  end)
end

local function ensure_session(callback)
  if state.session_id then
    callback(state.session_id, nil)
    return
  end
  table.insert(state.connection_waiters, callback)
  if state.connecting then return end
  state.connecting = true
  state.connection = "connecting"
  render()
  state.client:ensure_session(function(session_id, history, err)
    state.connecting = false
    if session_id then
      state.session_id = session_id
      state.connection = "connected"
      load_history_into_state(history)
      state.history_signature = history_signature(history)
    else
      state.connection = failed_connection(err)
    end
    local waiters = state.connection_waiters
    state.connection_waiters = {}
    for _, waiter in ipairs(waiters) do waiter(session_id, err) end
    render()
    if session_id then
      state.client:sessions(80, function(items)
        if state.session_id ~= session_id then return end
        for _, item in ipairs(items or {}) do
          local normalized = normalize_session(item)
          if normalized and normalized.session_id == session_id then
            state.session_name = normalized.display_name
            render()
            break
          end
        end
      end)
    end
  end)
end

local function event_text(event)
  local content_delta = nonempty_text(event.content_delta)
  if content_delta then return content_delta end
  local final_text = nonempty_text(event.final_text)
  if final_text then return final_text end
  return nil
end

local function update_tool(event)
  local name = nonempty_text(event.tool_name) or "tool"
  local key = nonempty_text(event.run_id) or nonempty_text(event.tool_run_id) or name
  if not state.tools[key] then table.insert(state.tool_order, key) end
  state.tools[key] = {
    name = name,
    status = nonempty_text(event.tool_status) or "running",
  }
end

local function handle_attention(event, force)
  local budget_request_id = nonempty_text(event.budget_request_id)
  local permission_request_id = nonempty_text(event.permission_request_id)
  local request_id = budget_request_id or permission_request_id
  if not request_id or (state.handled_requests[request_id] and not force) then return end
  state.handled_requests[request_id] = true
  state.pending_attention = event

  if budget_request_id then
    local requested_rounds = event.requested_rounds
    local rounds = type(requested_rounds) == "number" and requested_rounds
      or tonumber(nonempty_text(requested_rounds))
      or 1
    local message = "Medousa needs " .. rounds .. " more tool round" .. (rounds == 1 and "" or "s") .. " to finish."
    state.current_status = "waiting for your budget decision"
    if not force then table.insert(state.messages, { role = "attention", content = message }) end
    render()
    vim.ui.select({ "Approve", "Deny" }, { prompt = message }, function(choice)
      if not choice then
        state.current_status = "budget approval required · :MedousaAttention"
        render()
        return
      end
      local action = choice == "Approve" and "approve_budget" or "deny_budget"
      local callback = function(response, err)
        if not response and err then
          table.insert(state.messages, { role = "error", content = err })
        else
          state.pending_attention = nil
          state.current_status = choice == "Approve" and "budget approved" or "budget denied"
        end
        render()
      end
      if action == "approve_budget" then
        state.client:approve_budget(request_id, rounds, callback)
      else
        state.client:deny_budget(request_id, callback)
      end
    end)
  elseif permission_request_id then
    local message = nonempty_text(event.operator_message)
      or nonempty_text(event.message)
      or "Medousa needs permission to continue."
    state.current_status = "waiting for your permission decision"
    if not force then table.insert(state.messages, { role = "attention", content = message }) end
    render()
    vim.ui.select({ "Approve", "Deny" }, { prompt = message }, function(choice)
      if not choice then
        state.current_status = "permission approval required · :MedousaAttention"
        render()
        return
      end
      state.client:resolve_permission(request_id, choice == "Approve", function(response, err)
        if not response and err then
          table.insert(state.messages, { role = "error", content = err })
        else
          state.pending_attention = nil
          state.current_status = choice == "Approve" and "permission approved" or "permission denied"
        end
        render()
      end)
    end)
  end
end

normalize_session = function(item)
  local id = util.trim(tostring(item.session_id or item.id or ""))
  if id == "" then return nil end
  local preview = type(item.preview) == "string" and item.preview or ""
  local name = type(item.display_name) == "string" and util.trim(item.display_name) or ""
  return {
    session_id = id,
    display_name = name ~= "" and name or util.first_line(preview) ~= "" and util.first_line(preview) or "New conversation",
    preview = util.first_line(preview),
    turns = tonumber(item.turns) or 0,
    last_timestamp = item.last_timestamp,
  }
end

local function normalize_project(item)
  local id = util.trim(tostring(item.id or item.work_id or ""))
  if id == "" then return nil end
  local environment = type(item.environment) == "table" and item.environment or {}
  local title = util.trim(tostring(item.title or ""))
  return {
    id = id,
    title = title ~= "" and title or id,
    brief = type(item.brief) == "string" and item.brief or "",
    state = tostring(item.state or ""),
    human_phase = type(item.human_phase) == "string" and item.human_phase or "",
    worktree = type(item.worktree) == "string" and item.worktree or environment.worktree,
  }
end

local function reset_runtime_state()
  state.agent_mode = "general"
  state.mode_label = "General"
  state.binding_work_id = nil
  state.project = nil
end

refresh_runtime_state = function(session_id, callback)
  if not state.client or not session_id then
    if callback then callback("No active Medousa conversation") end
    return
  end
  state.client:agent_mode(session_id, function(mode, mode_err)
    if state.session_id ~= session_id then return end
    if not mode then
      if callback then callback(mode_err) end
      return
    end
    state.agent_mode = mode.effective_mode or "general"
    state.mode_label = state.agent_mode == "coder" and "Coder" or "General"
    state.client:code_binding(session_id, function(binding, binding_err)
      if state.session_id ~= session_id then return end
      if not binding then
        if callback then callback(binding_err) end
        return
      end
      state.binding_work_id = binding.work_id ~= vim.NIL and binding.work_id or nil
      state.project = nil
      if not state.binding_work_id then
        render()
        if callback then callback(nil) end
        return
      end
      state.client:forge_item(state.binding_work_id, function(item)
        if state.session_id ~= session_id then return end
        state.project = item and normalize_project(item) or {
          id = state.binding_work_id,
          title = state.binding_work_id,
          brief = "",
          state = "ready",
        }
        render()
        if callback then callback(nil) end
      end)
    end)
  end)
end

local function offer_open_worktree(project, callback)
  local worktree = project and project.worktree
  local stat = type(worktree) == "string" and vim.uv.fs_stat(worktree) or nil
  if not stat or stat.type ~= "directory" then
    if callback then callback() end
    return
  end
  vim.ui.select({ "Open governed worktree", "Keep current editor directory" }, {
    prompt = "“" .. project.title .. "” is bound. Open its Forge worktree?",
  }, function(choice)
    if choice == "Open governed worktree" then
      vim.cmd("cd " .. vim.fn.fnameescape(worktree))
      notify("Opened " .. project.title .. " at " .. worktree)
    end
    if callback then callback() end
  end)
end

local function bind_project(project, callback)
  local session_id = state.session_id
  state.client:set_code_binding(session_id, project.id, function(response, err)
    if not response then
      notify(err or "Could not bind that project", vim.log.levels.ERROR)
      if callback then callback(false) end
      return
    end
    state.binding_work_id = project.id
    state.project = project
    render()
    offer_open_worktree(project, function()
      if callback then callback(true) end
    end)
  end)
end

local function create_project(callback)
  local session_id = state.session_id
  vim.ui.input({ prompt = "Project name: " }, function(title)
    title = util.trim(title)
    if title == "" then
      if callback then callback(false) end
      return
    end
    vim.ui.input({ prompt = "What should Medousa build? (optional): " }, function(brief)
      if brief == nil then
        if callback then callback(false) end
        return
      end
      state.current_status = "creating “" .. title .. "”"
      render()
      state.client:start_code_project(session_id, {
        title = title,
        brief = util.trim(brief) ~= "" and util.trim(brief) or title,
        source = "blank",
      }, function(created, err)
        state.current_status = nil
        if state.session_id ~= session_id then return end
        if not created then
          notify(err or "Could not create the project", vim.log.levels.ERROR)
          render()
          if callback then callback(false) end
          return
        end
        local project = normalize_project(created)
        state.binding_work_id = created.work_id
        state.project = project
        render()
        notify("Created and bound “" .. project.title .. "”.")
        offer_open_worktree(project, function()
          if callback then callback(true) end
        end)
      end)
    end)
  end)
end

local function choose_project(options, callback)
  options = options or {}
  local session_id = state.session_id
  state.current_status = "loading projects"
  render()
  state.client:forge_items(function(items, err)
    if state.session_id ~= session_id then return end
    state.current_status = nil
    if not items then
      notify(err or "Could not load projects", vim.log.levels.ERROR)
      render()
      if callback then callback(false) end
      return
    end
    local choices = {}
    for _, item in ipairs(items) do
      local project = normalize_project(item)
      if project and (project.state:lower() == "ready" or project.state:lower() == "executing") and project.worktree then
        table.insert(choices, {
          label = project.title .. (project.id == state.binding_work_id and " · bound" or " · ready"),
          action = "bind",
          project = project,
        })
      end
    end
    table.insert(choices, { label = "+ Create a new project", action = "create" })
    if options.allow_agent_setup then
      table.insert(choices, {
        label = "✦ Let Medousa choose or create it from this message",
        action = "agent",
      })
    end
    if state.binding_work_id then
      table.insert(choices, { label = "× Stop following this project", action = "detach" })
    end
    render()
    picker.projects(choices, function(choice)
      if not choice then
        if callback then callback(false) end
      elseif choice.action == "bind" then
        bind_project(choice.project, callback)
      elseif choice.action == "create" then
        create_project(callback)
      elseif choice.action == "agent" then
        if callback then callback(true, true) end
      elseif choice.action == "detach" then
        state.client:clear_code_binding(session_id, function(response, detach_err)
          if not response then
            notify(detach_err or "Could not detach the project", vim.log.levels.ERROR)
            if callback then callback(false) end
            return
          end
          state.binding_work_id, state.project = nil, nil
          render()
          if callback then callback(false) end
        end)
      end
    end)
  end)
end

check_mode_proposals = function(session_id)
  if not state.client or state.session_id ~= session_id then return end
  state.client:mode_proposals(session_id, function(response)
    if state.session_id ~= session_id or not response then return end
    local pending
    for _, proposal in ipairs(response.proposals or {}) do
      if proposal.status == "pending" and not state.handled_mode_proposals[proposal.proposal_id] then
        pending = proposal
      end
    end
    if not pending then return end
    state.handled_mode_proposals[pending.proposal_id] = true
    local target = pending.to_mode == "coder" and "Coder" or "General"
    vim.ui.select({ "Switch to " .. target, "Not now" }, {
      prompt = "Medousa suggests " .. target .. ": " .. tostring(pending.reason or "better fit"),
    }, function(choice)
      if not choice then return end
      local accept = choice == "Switch to " .. target
      state.client:decide_mode_proposal(session_id, pending.proposal_id, accept, function(decision, err)
        if not decision then
          notify(err or "That mode suggestion expired", vim.log.levels.WARN)
          return
        end
        refresh_runtime_state(session_id, function()
          if accept and pending.to_mode == "coder" and not state.binding_work_id then M.projects() end
        end)
      end)
    end)
  end)
end

function M.toggle()
  if valid_window(state.chat_win) then
    close_room()
    return
  end
  local value = capture_current()
  create_room(value)
  ensure_session(function(session_id, err)
    if not session_id then
      notify(err or "Workshop unavailable", vim.log.levels.ERROR)
      return
    end
    refresh_runtime_state(session_id, function()
      check_mode_proposals(session_id)
      focus_prompt()
    end)
  end)
end

local function run_turn(session_id, prompt, value, setup_authorized)
  state.last_context = value
  state.last_prompt = prompt
  state.answer = ""
  state.last_answer = ""
  state.current_status = "thinking"
  state.tools, state.tool_order = {}, {}
  state.handled_requests = {}
  state.pending_attention = nil
  state.connection = "connected"
  table.insert(state.messages, { role = "user", content = prompt, context_label = context.describe(value) })
  render()
  state.client:turn(session_id, prompt, context.host_context(value), {
    on_status = function(text)
      state.connection = "recovering"
      state.current_status = text
      render()
    end,
    on_event = function(event)
      state.connection = "connected"
      handle_attention(event)
      local text = event_text(event)
      local operator_message = nonempty_text(event.operator_message)
      if operator_message then
        state.current_status = operator_message
      end
      if nonempty_text(event.tool_name) then update_tool(event) end
      if text then
        if nonempty_text(event.final_text) then state.answer = text else state.answer = text_value(state.answer) .. text end
      end
      render()
    end,
    on_handoff = function(event)
      state.answer = nil
      state.busy = false
      state.pending_attention = nil
      state.current_status = "workshop is running · you can keep typing"
      state.connection = "connected"
      render()
      focus_prompt()
      refresh_runtime_state(session_id, function() check_mode_proposals(session_id) end)
      vim.defer_fn(function()
        poll_workshop_history(session_id, 90)
      end, 500)
    end,
    on_done = function(event)
      local answer = text_value(state.answer)
      state.last_answer = answer ~= "" and answer or (nonempty_text(event.final_text) or "")
      if state.last_answer ~= "" then
        table.insert(state.messages, { role = "assistant", content = state.last_answer })
      end
      state.answer = nil
      state.busy = false
      state.pending_attention = nil
      state.current_status = nil
      state.connection = "connected"
      render()
      focus_prompt()
      refresh_runtime_state(session_id, function() check_mode_proposals(session_id) end)
    end,
    on_error = function(err_value)
      state.answer = nil
      state.busy = false
      state.pending_attention = nil
      state.current_status = nil
      state.connection = failed_connection(err_value)
      table.insert(state.messages, { role = "error", content = nonempty_text(err_value) or "Unknown failure" })
      render()
      refresh_runtime_state(session_id)
    end,
  }, { code_project_setup_authorized = setup_authorized == true })
end

local function restore_unsent_prompt(prompt, value)
  state.busy = false
  state.current_status = nil
  state.connection = "connected"
  state.drafts[session_key()] = prompt
  state.draft_contexts[session_key()] = value
  state.pending_context = value
  set_prompt(prompt)
  render()
  focus_prompt()
end

function M.send(prompt, explicit_context)
  prompt = util.trim(prompt)
  if prompt == "" then return end
  if state.busy then
    notify("A response is already in progress; press c or Ctrl-C to stop it.", vim.log.levels.WARN)
    return
  end
  local value = explicit_context or capture_current()
  create_room(value)
  state.busy = true
  state.connection = "connecting"
  state.current_status = "opening the workshop"
  render()
  ensure_session(function(session_id, err)
    if not session_id then
      restore_unsent_prompt(prompt, value)
      state.connection = failed_connection(err)
      table.insert(state.messages, { role = "error", content = err or "Workshop unavailable" })
      render()
      return
    end
    state.session_id = session_id
    refresh_runtime_state(session_id, function(runtime_err)
      if runtime_err then
        restore_unsent_prompt(prompt, value)
        table.insert(state.messages, { role = "error", content = runtime_err })
        render()
        return
      end
      if state.agent_mode ~= "coder" or state.binding_work_id then
        run_turn(session_id, prompt, value)
        return
      end
      state.current_status = "choose or create a Coder project"
      render()
      choose_project({ allow_agent_setup = true }, function(ready, setup_authorized)
        if ready then
          run_turn(session_id, prompt, value, setup_authorized)
        else
          restore_unsent_prompt(prompt, value)
        end
      end)
    end)
  end)
end

function M.ask(prefill, value)
  value = value or capture_current()
  state.pending_context = value
  create_room(value)
  if prefill then set_prompt(prefill) end
  ensure_session(function() focus_prompt() end)
end

function M.ask_selection()
  M.ask(nil, context.current())
end

function M.explain(value)
  value = value or capture_current()
  if not value.selection then
    M.ask("Explain the current buffer and identify the most important improvement.", value)
    return
  end
  M.send("Explain this selection and point out the most important improvement.", value)
end

function M.fix(value)
  value = value or capture_current()
  if #value.diagnostics == 0 then
    M.ask("Review the current buffer for likely bugs and propose a focused change.", value)
    return
  end
  M.send("Help me fix the diagnostics in this buffer. Show a focused change and explain why it fixes each diagnostic.", value)
end

function M.retry()
  if state.busy then return end
  if not state.last_prompt then
    notify("There is no previous prompt to retry.", vim.log.levels.WARN)
    return
  end
  M.send(state.last_prompt, capture_current())
end

function M.cancel()
  if not state.busy or not state.session_id then return end
  state.current_status = "stopping"
  render()
  state.client:cancel(state.session_id, function(err)
    state.busy = false
    state.answer = nil
    state.pending_attention = nil
    state.current_status = nil
    if err then
      table.insert(state.messages, { role = "error", content = err })
    else
      table.insert(state.messages, { role = "assistant", content = "Response stopped." })
    end
    render()
    focus_prompt()
  end)
end

function M.attention()
  if not state.pending_attention then
    notify("Medousa is not waiting for a decision.")
    return
  end
  handle_attention(state.pending_attention, true)
end

function M.new_session()
  if state.busy then
    notify("Stop the active response before starting a new conversation.", vim.log.levels.WARN)
    return
  end
  save_draft()
  state.connection = "connecting"
  state.client:create_session(function(created, err)
    if not created then
      state.connection = "unavailable"
      notify(err or "Could not create a new conversation", vim.log.levels.ERROR)
      render()
      return
    end
    state.session_id = created.session_id
    state.session_name = created.display_name or "New conversation"
    reset_runtime_state()
    util.write_session(state.session_id)
    state.messages = {}
    state.last_answer = ""
    state.last_prompt = nil
    state.last_context = nil
    state.answer = nil
    state.history_signature = ""
    state.current_status = nil
    state.connection = "connected"
    set_prompt(state.drafts[session_key()] or "")
    state.pending_context = state.draft_contexts[session_key()]
    render()
    refresh_runtime_state(state.session_id, function() focus_prompt() end)
  end)
end

function M.switch_session(item)
  if not item or item.session_id == state.session_id then
    focus_prompt()
    return
  end
  if state.busy then
    notify("Stop the active response before switching conversations.", vim.log.levels.WARN)
    return
  end
  save_draft()
  state.connection = "connecting"
  state.current_status = "opening “" .. item.display_name .. "”"
  render()
  state.client:history(item.session_id, function(history, err)
    if not history then
      state.connection = "unavailable"
      state.current_status = nil
      notify(err or "Could not open that conversation", vim.log.levels.ERROR)
      render()
      return
    end
    state.session_id = item.session_id
    state.session_name = item.display_name
    reset_runtime_state()
    util.write_session(state.session_id)
    state.messages = {}
    state.last_answer = ""
    state.last_prompt = nil
    state.last_context = nil
    state.history_signature = history_signature(history)
    for _, turn in ipairs(history.turns or {}) do
      if turn.role == "user" or turn.role == "assistant" then
        local content_value = turn.content
        table.insert(state.messages, { role = turn.role, content = content_value })
        if turn.role == "assistant" then state.last_answer = content_value end
        if turn.role == "user" then state.last_prompt = content_value end
      end
    end
    state.connection = "connected"
    state.current_status = nil
    set_prompt(state.drafts[session_key()] or "")
    state.pending_context = state.draft_contexts[session_key()]
    render()
    refresh_runtime_state(state.session_id, function()
      check_mode_proposals(state.session_id)
      focus_prompt()
    end)
  end)
end

function M.sessions()
  if state.busy then
    notify("Stop the active response before switching conversations.", vim.log.levels.WARN)
    return
  end
  create_room(state.room_context or capture_current())
  state.current_status = "loading conversations"
  render()
  state.client:sessions(80, function(items, err)
    state.current_status = nil
    if not items then
      notify(err or "Could not load conversations", vim.log.levels.ERROR)
      render()
      return
    end
    local normalized = {}
    for _, item in ipairs(items) do
      local value = normalize_session(item)
      if value then
        table.insert(normalized, value)
        if value.session_id == state.session_id then state.session_name = value.display_name end
      end
    end
    render()
    if #normalized == 0 then
      notify("No conversations yet.")
      return
    end
    picker.sessions(normalized, M.switch_session)
  end)
end

function M.select_mode()
  if state.busy then
    notify("Stop the active response before changing modes.", vim.log.levels.WARN)
    return
  end
  create_room(state.room_context or capture_current())
  ensure_session(function(session_id, err)
    if not session_id then
      notify(err or "Workshop unavailable", vim.log.levels.ERROR)
      return
    end
    state.current_status = "loading modes"
    render()
    state.client:agent_modes(function(response, modes_err)
      state.current_status = nil
      if not response then
        notify(modes_err or "Could not load Medousa modes", vim.log.levels.ERROR)
        render()
        return
      end
      local choices = {}
      for _, mode in ipairs(response.modes or response) do
        if mode.available then
          local selected = mode.mode == state.agent_mode and " · active" or ""
          table.insert(choices, {
            label = tostring(mode.label or mode.mode) .. selected,
            mode = mode.mode,
          })
        end
      end
      render()
      picker.modes(choices, function(choice)
        if not choice then return end
        if choice.mode == state.agent_mode then
          focus_prompt()
          return
        end
        state.client:set_agent_mode(session_id, choice.mode, function(updated, update_err)
          if not updated then
            notify(update_err or "Could not change Medousa mode", vim.log.levels.ERROR)
            return
          end
          refresh_runtime_state(session_id, function()
            notify("Switched to " .. state.mode_label .. ".")
            if state.agent_mode == "coder" and not state.binding_work_id then
              M.projects()
            else
              focus_prompt()
            end
          end)
        end)
      end)
    end)
  end)
end

function M.projects()
  if state.busy then
    notify("Stop the active response before changing projects.", vim.log.levels.WARN)
    return
  end
  create_room(state.room_context or capture_current())
  ensure_session(function(session_id, err)
    if not session_id then
      notify(err or "Workshop unavailable", vim.log.levels.ERROR)
      return
    end
    refresh_runtime_state(session_id, function(runtime_err)
      if runtime_err then
        notify(runtime_err, vim.log.levels.ERROR)
        return
      end
      choose_project({}, function() focus_prompt() end)
    end)
  end)
end

function M.rename_session()
  if not state.session_id then
    notify("Open a conversation first.", vim.log.levels.WARN)
    return
  end
  local session_id = state.session_id
  vim.ui.input({ prompt = "Conversation name: ", default = state.session_name or "" }, function(value)
    value = util.trim(value)
    if value == "" then return end
    state.client:rename_session(session_id, value, function(response, err)
      if not response then
        notify(err or "Could not rename the conversation", vim.log.levels.ERROR)
        return
      end
      if state.session_id == session_id then state.session_name = value end
      render()
      notify("Conversation renamed.")
    end)
  end)
end

function M.delete_session()
  if not state.session_id then return end
  if state.busy then
    notify("Stop the active response before deleting this conversation.", vim.log.levels.WARN)
    return
  end
  local label = state.session_name or "this conversation"
  local answer = vim.fn.confirm("Delete “" .. label .. "” and its Medousa memory? This cannot be undone.", "&Delete\n&Cancel", 2)
  if answer ~= 1 then return end
  local deleting = state.session_id
  state.client:delete_session(deleting, function(response, err)
    if not response then
      notify(err or "Could not delete the conversation", vim.log.levels.ERROR)
      return
    end
    state.session_id, state.session_name = nil, nil
    reset_runtime_state()
    state.messages, state.last_answer, state.last_prompt = {}, "", nil
    util.clear_session()
    render()
    M.new_session()
  end)
end

function M.copy_last()
  if state.last_answer == "" then
    notify("There is no completed answer to copy.", vim.log.levels.WARN)
    return
  end
  util.copy(state.last_answer)
  notify("Copied the last answer.")
end

function M.copy_code()
  local blocks = stream.extract_code_blocks(state.last_answer)
  if #blocks == 0 then
    notify("The last answer did not contain a fenced code block.", vim.log.levels.WARN)
    return
  end
  local cursor_block = code_block_at_cursor()
  local function copy(block)
    util.copy(block.text)
    notify("Copied the " .. block.language .. " code block.")
  end
  if cursor_block then
    for _, block in ipairs(blocks) do
      if block.text == cursor_block.text and block.language == cursor_block.language then
        copy(block)
        return
      end
    end
  end
  if #blocks == 1 then
    copy(blocks[1])
  else
    vim.ui.select(blocks, { prompt = "Choose code to copy:", format_item = edit.block_label }, function(choice)
      if choice then copy(choice) end
    end)
  end
end

function M.apply()
  if state.last_answer == "" then
    notify("There is no completed code answer to preview.", vim.log.levels.WARN)
    return
  end
  if not state.last_context then
    notify("Open a fresh editor request before applying code from this conversation.", vim.log.levels.WARN)
    return
  end
  local blocks = stream.extract_code_blocks(state.last_answer)
  if #blocks == 0 then
    notify("The last answer did not contain a fenced code block.", vim.log.levels.WARN)
    return
  end
  local function choose_target(block)
    edit.choose_target(state.last_context, function(target)
      local prepared, err = edit.prepare(block, state.last_context, target)
      if not prepared then
        notify(err, vim.log.levels.WARN)
        return
      end
      edit.preview(prepared, {
        on_copy = function() notify("Copied the proposed code.") end,
        on_error = function(message) notify(message, vim.log.levels.WARN) end,
        on_apply = function(applied)
          state.last_context = nil
          notify("Applied " .. applied.language .. " code as one undoable edit.")
          if valid_window(state.origin_win) then vim.api.nvim_set_current_win(state.origin_win) end
        end,
      }, { border = defaults.border })
    end)
  end
  local cursor_block = code_block_at_cursor()
  if cursor_block then
    for _, block in ipairs(blocks) do
      if block.text == cursor_block.text and block.language == cursor_block.language then
        choose_target(block)
        return
      end
    end
  end
  if #blocks == 1 then
    choose_target(blocks[1])
  else
    vim.ui.select(blocks, { prompt = "Choose code to preview:", format_item = edit.block_label }, function(choice)
      if choice then choose_target(choice) end
    end)
  end
end

function M.diagnostics()
  local value = state.last_context or capture_current()
  if #value.diagnostics == 0 then
    notify("There are no diagnostics in the captured buffer.")
    return
  end
  local items = {}
  for _, diagnostic in ipairs(value.diagnostics) do
    table.insert(items, {
      bufnr = value.buffer,
      lnum = diagnostic.range.start.line + 1,
      col = diagnostic.range.start.character + 1,
      text = diagnostic.message,
      type = diagnostic.severity == "error" and "E" or diagnostic.severity == "warning" and "W" or "I",
    })
  end
  vim.fn.setqflist({}, " ", { title = "Medousa context diagnostics", items = items })
  vim.cmd("copen")
end

function M.operator()
  vim.go.operatorfunc = "v:lua.require'medousa'._operator"
  return "g@"
end

function M._operator()
  local buffer = vim.api.nvim_get_current_buf()
  M.ask(nil, context.from_operator(buffer))
end

function M.statusline()
  if not state.configured then return "" end
  if state.busy then return "Medousa:think" end
  if state.connection == "recovering" then return "Medousa:recover" end
  if state.connection == "connected" then return "Medousa:on" end
  if state.connection == "unavailable" then return "Medousa:off" end
  if state.connection == "unauthorized" then return "Medousa:auth" end
  return "Medousa"
end

local function command(name, callback, options)
  pcall(vim.api.nvim_del_user_command, name)
  vim.api.nvim_create_user_command(name, callback, options or {})
end

function M.setup(options)
  if vim.fn.has("nvim-0.10") ~= 1 then error("Medousa requires Neovim 0.10 or newer") end
  options = vim.tbl_deep_extend("force", defaults, options or {})
  defaults = options
  state.client = Client.new(options)
  state.configured = true
  if vim.fn.executable("curl") ~= 1 then
    notify("curl is required to connect to the Medousa workshop.", vim.log.levels.ERROR)
  end

  command("MedousaToggle", M.toggle, { desc = "Toggle Medousa coding room" })
  command("MedousaAsk", function(command_options)
    local value = command_options.range > 0
      and context.from_range(vim.api.nvim_get_current_buf(), command_options.line1, command_options.line2)
      or nil
    if util.trim(command_options.args) == "" then M.ask(nil, value) else M.send(command_options.args, value) end
  end, { range = true, nargs = "*", desc = "Ask Medousa about the current code or range" })
  command("MedousaExplain", function(command_options)
    local value = command_options.range > 0
      and context.from_range(vim.api.nvim_get_current_buf(), command_options.line1, command_options.line2)
      or nil
    M.explain(value)
  end, { range = true, desc = "Explain the current code or range with Medousa" })
  command("MedousaFix", function() M.fix() end, { desc = "Ask Medousa to fix current diagnostics" })
  command("MedousaApply", M.apply, { desc = "Preview and apply code from the last Medousa answer" })
  command("MedousaNew", M.new_session, { desc = "Start a new Medousa conversation" })
  command("MedousaCancel", M.cancel, { desc = "Stop the active Medousa response" })
  command("MedousaSessions", M.sessions, { desc = "Open Medousa conversation history" })
  command("MedousaMode", M.select_mode, { desc = "Select the active Medousa mode" })
  command("MedousaProject", M.projects, { desc = "Choose, create, or detach a Coder project" })
  command("MedousaRename", M.rename_session, { desc = "Rename the active Medousa conversation" })
  command("MedousaDelete", M.delete_session, { desc = "Delete the active Medousa conversation" })
  command("MedousaDiagnostics", M.diagnostics, { desc = "Open captured diagnostics in the quickfix list" })
  command("MedousaAttention", M.attention, { desc = "Resolve a pending Medousa approval request" })
  command("MedousaStatus", function()
    state.client:health(function(_, err)
      state.connection = err and "unavailable" or "connected"
      render()
      notify(err and "Workshop unavailable" or "Workshop connected")
    end)
  end, { desc = "Check Medousa workshop connection" })

  for name, mapping in pairs(options.keymaps or {}) do
    if mapping and mapping ~= false then
      if name == "toggle" then
        vim.keymap.set("n", mapping, M.toggle, { silent = true, desc = "Medousa coding room" })
      elseif name == "ask" then
        vim.keymap.set("n", mapping, function() M.ask() end, { silent = true, desc = "Ask Medousa" })
        vim.keymap.set("x", mapping, M.ask_selection, { silent = true, desc = "Ask Medousa about selection" })
      elseif name == "explain" then
        vim.keymap.set({ "n", "x" }, mapping, M.explain, { silent = true, desc = "Explain with Medousa" })
      elseif name == "fix" then
        vim.keymap.set("n", mapping, M.fix, { silent = true, desc = "Fix with Medousa" })
      elseif name == "operator" then
        vim.keymap.set("n", mapping, M.operator, { expr = true, silent = true, desc = "Ask Medousa about motion" })
      elseif name == "sessions" then
        vim.keymap.set("n", mapping, M.sessions, { silent = true, desc = "Medousa conversations" })
      elseif name == "mode" then
        vim.keymap.set("n", mapping, M.select_mode, { silent = true, desc = "Select Medousa mode" })
      elseif name == "project" then
        vim.keymap.set("n", mapping, M.projects, { silent = true, desc = "Select Medousa project" })
      end
    end
  end
end

return M
