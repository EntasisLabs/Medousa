local Client = require("medousa.client")
local context = require("medousa.context")
local stream = require("medousa.stream")
local util = require("medousa.util")

local M = {}
local state = {
  client = nil,
  session_id = nil,
  chat_buf = nil,
  chat_win = nil,
  prompt_buf = nil,
  prompt_win = nil,
  messages = {},
  answer = nil,
  busy = false,
  last_context = nil,
  last_answer = "",
  configured = false,
}

local defaults = {
  endpoint = "http://127.0.0.1:7419",
  token = nil,
  width = 0.72,
  height = 0.68,
  keymaps = {
    toggle = "<leader>mc",
    ask = "<leader>ma",
    explain = "<leader>me",
    fix = "<leader>mf",
  },
}

local function valid_buffer(buffer)
  return buffer and vim.api.nvim_buf_is_valid(buffer)
end

local function close_window(window)
  if window and vim.api.nvim_win_is_valid(window) then
    vim.api.nvim_win_close(window, true)
  end
end

local function set_lines(buffer, lines)
  if not valid_buffer(buffer) then return end
  vim.bo[buffer].modifiable = true
  vim.api.nvim_buf_set_lines(buffer, 0, -1, false, lines)
  vim.bo[buffer].modifiable = false
end

local function message_lines()
  local lines = { "  Medousa · coding room", "  " .. (state.session_id or "connecting"), "" }
  for _, message in ipairs(state.messages) do
    if message.role == "user" then
      table.insert(lines, "You · " .. (message.context_label or "current code"))
      vim.list_extend(lines, vim.split(message.content, "\n", { plain = true }))
      table.insert(lines, "")
    elseif message.role == "assistant" then
      table.insert(lines, "Medousa")
      vim.list_extend(lines, vim.split(message.content, "\n", { plain = true }))
      table.insert(lines, "")
    elseif message.role == "status" then
      table.insert(lines, "· " .. message.content)
    elseif message.role == "tool" then
      table.insert(lines, "  · " .. message.content)
    end
  end
  if state.answer then
    table.insert(lines, "Medousa · thinking")
    vim.list_extend(lines, vim.split(state.answer, "\n", { plain = true }))
  end
  return lines
end

local function render()
  set_lines(state.chat_buf, message_lines())
  if state.chat_win and vim.api.nvim_win_is_valid(state.chat_win) then
    vim.api.nvim_win_set_cursor(state.chat_win, { vim.api.nvim_buf_line_count(state.chat_buf), 0 })
  end
end

local function notify(message, level)
  vim.schedule(function()
    vim.notify("Medousa: " .. message, level or vim.log.levels.INFO)
  end)
end

local function focus_prompt()
  if state.prompt_win and vim.api.nvim_win_is_valid(state.prompt_win) then
    vim.api.nvim_set_current_win(state.prompt_win)
    vim.cmd("startinsert")
  end
end

local function create_floats()
  if state.chat_win and vim.api.nvim_win_is_valid(state.chat_win) then
    focus_prompt()
    return
  end
  local width = math.max(48, math.floor(vim.o.columns * defaults.width))
  local height = math.max(12, math.floor(vim.o.lines * defaults.height))
  local row = math.max(1, math.floor((vim.o.lines - height) / 2) - 1)
  local col = math.max(1, math.floor((vim.o.columns - width) / 2))
  state.chat_buf = vim.api.nvim_create_buf(false, true)
  state.chat_win = vim.api.nvim_open_win(state.chat_buf, true, {
    relative = "editor", row = row, col = col, width = width, height = height,
    style = "minimal", border = "rounded", title = " Medousa ", title_pos = "center",
  })
  vim.bo[state.chat_buf].filetype = "markdown"
  vim.bo[state.chat_buf].modifiable = false
  vim.wo[state.chat_win].wrap = true
  vim.wo[state.chat_win].linebreak = true
  vim.wo[state.chat_win].cursorline = true

  state.prompt_buf = vim.api.nvim_create_buf(false, true)
  local prompt_height = 3
  state.prompt_win = vim.api.nvim_open_win(state.prompt_buf, false, {
    relative = "editor", row = row + height + 1, col = col, width = width, height = prompt_height,
    style = "minimal", border = "rounded", title = " Ask Medousa · Enter to send · q to close ", title_pos = "left",
  })
  vim.bo[state.prompt_buf].buftype = "prompt"
  vim.bo[state.prompt_buf].bufhidden = "wipe"
  vim.fn.prompt_setprompt(state.prompt_buf, "> ")
  vim.fn.prompt_setcallback(state.prompt_buf, function(value)
    if util.trim(value) ~= "" then
      M.send(value)
    end
  end)
  vim.keymap.set("n", "q", M.toggle, { buffer = state.chat_buf, silent = true, desc = "Close Medousa" })
  vim.keymap.set("n", "q", M.toggle, { buffer = state.prompt_buf, silent = true, desc = "Close Medousa" })
  vim.keymap.set("n", "a", focus_prompt, { buffer = state.chat_buf, silent = true, desc = "Ask Medousa" })
  vim.keymap.set("n", "a", focus_prompt, { buffer = state.prompt_buf, silent = true, desc = "Ask Medousa" })
  vim.keymap.set("n", "A", M.apply, { buffer = state.chat_buf, silent = true, desc = "Apply Medousa code" })
  vim.keymap.set("n", "c", M.cancel, { buffer = state.chat_buf, silent = true, desc = "Cancel Medousa" })
  vim.api.nvim_create_autocmd("VimResized", { buffer = state.chat_buf, callback = function() if state.chat_win and vim.api.nvim_win_is_valid(state.chat_win) then vim.api.nvim_win_set_config(state.chat_win, { width = math.max(48, math.floor(vim.o.columns * defaults.width)), height = math.max(12, math.floor(vim.o.lines * defaults.height)), col = math.max(1, math.floor((vim.o.columns - math.max(48, math.floor(vim.o.columns * defaults.width))) / 2)) }) end end })
  render()
end

function M.toggle()
  if state.chat_win and vim.api.nvim_win_is_valid(state.chat_win) then
    close_window(state.prompt_win)
    close_window(state.chat_win)
    state.chat_win, state.prompt_win = nil, nil
    return
  end
  create_floats()
  if not state.session_id then
    state.messages = { { role = "status", content = "Connecting to Medousa…" } }
    render()
    state.client:ensure_session(function(session_id, history, err)
      if not session_id then
        notify(err or "Workshop unavailable", vim.log.levels.ERROR)
        return
      end
      state.session_id = session_id
      state.messages = {}
      for _, turn in ipairs(history.turns or {}) do
        if turn.role == "user" or turn.role == "assistant" then
          table.insert(state.messages, { role = turn.role, content = turn.content })
        end
      end
      render()
      focus_prompt()
    end)
  else
    focus_prompt()
  end
end

local function event_text(event)
  if event.content_delta and event.content_delta ~= "" then return event.content_delta end
  if event.final_text and event.final_text ~= "" then return event.final_text end
  return nil
end

function M.send(prompt)
  if state.busy then
    notify("A response is already in progress; press c to stop it.", vim.log.levels.WARN)
    return
  end
  create_floats()
  state.busy = true
  state.last_context = context.current()
  state.answer = ""
  state.last_answer = ""
  table.insert(state.messages, { role = "user", content = prompt, context_label = context.describe(state.last_context) })
  render()
  state.client:ensure_session(function(session_id, _, err)
    if not session_id then
      state.busy = false
      notify(err or "Workshop unavailable", vim.log.levels.ERROR)
      return
    end
    state.session_id = session_id
    state.client:turn(session_id, prompt, context.supplement(state.last_context), {
      on_status = function(text)
        table.insert(state.messages, { role = "status", content = text })
        render()
      end,
      on_event = function(event)
        local text = event_text(event)
        if event.operator_message and event.operator_message ~= "" then
          table.insert(state.messages, { role = "status", content = event.operator_message })
        elseif event.tool_name and event.tool_name ~= "" then
          table.insert(state.messages, { role = "tool", content = event.tool_name .. " · " .. (event.tool_status or "running") })
        elseif text then
          if event.final_text then
            state.answer = text
          else
            state.answer = (state.answer or "") .. text
          end
        end
        render()
      end,
      on_done = function(event)
        state.last_answer = state.answer or event.final_text or ""
        if state.last_answer ~= "" then
          table.insert(state.messages, { role = "assistant", content = state.last_answer })
        end
        state.answer = nil
        state.busy = false
        render()
        focus_prompt()
      end,
      on_error = function(err)
        state.answer = nil
        state.busy = false
        table.insert(state.messages, { role = "status", content = "Error: " .. (err or "unknown failure") })
        render()
      end,
    })
  end)
end

function M.ask(prefill)
  create_floats()
  if prefill and state.prompt_buf then
    vim.api.nvim_buf_set_lines(state.prompt_buf, 0, -1, false, { prefill })
  end
  focus_prompt()
end

function M.explain()
  local value = context.current()
  if not value.selection then
    return M.ask("Explain the current buffer")
  end
  M.send("Explain this selection and point out the most important improvement.")
end

function M.fix()
  if #context.current().diagnostics == 0 then
    return M.ask("Review the current buffer for likely bugs")
  end
  M.send("Help me fix the diagnostics in this buffer. Show a focused change and explain why it fixes each diagnostic.")
end

function M.new_session()
  if state.busy then
    if vim.fn.confirm("Stop the active response and start a new conversation?", "&New session\n&Cancel", 2) ~= 1 then
      return
    end
    M.cancel()
  end
  state.client:create_session(function(created, err)
    if not created then
      notify(err or "Could not create a new conversation", vim.log.levels.ERROR)
      return
    end
    state.session_id = created.session_id
    util.write_session(state.session_id)
    state.messages = {}
    state.last_answer = ""
    state.answer = nil
    render()
    focus_prompt()
  end)
end

function M.cancel()
  if not state.busy or not state.session_id then return end
  state.client:cancel(state.session_id, function()
    state.busy = false
    state.answer = nil
    table.insert(state.messages, { role = "status", content = "Response stopped" })
    render()
    focus_prompt()
  end)
end

function M.apply()
  if state.last_answer == "" then
    notify("There is no completed code answer to apply.", vim.log.levels.WARN)
    return
  end
  local blocks = stream.extract_code_blocks(state.last_answer)
  if #blocks == 0 then
    notify("The last answer did not contain a fenced code block.", vim.log.levels.WARN)
    return
  end
  local function apply_block(block)
    local value = state.last_context
    if not value or not valid_buffer(value.buffer) then
      notify("The original buffer is no longer available.", vim.log.levels.ERROR)
      return
    end
    if value.changedtick ~= vim.api.nvim_buf_get_changedtick(value.buffer) then
      notify("The buffer changed since this answer was requested; ask again before applying it.", vim.log.levels.WARN)
      return
    end
    local question = value.selection and "Replace the original selection with this code?" or "Insert this code at the cursor?"
    if vim.fn.confirm(question, "&Apply\n&Cancel", 2) ~= 1 then return end
    local lines = vim.split(block.text, "\n", { plain = true })
    if value.selection_start and value.selection_end then
      vim.api.nvim_buf_set_lines(value.buffer, value.selection_start - 1, value.selection_end, false, lines)
    else
      local position = vim.api.nvim_win_get_cursor(0)[1]
      vim.api.nvim_buf_set_lines(value.buffer, position, position, false, lines)
    end
    notify("Applied " .. block.language .. " code to the buffer.")
  end
  if #blocks == 1 then
    apply_block(blocks[1])
  else
    vim.ui.select(blocks, { prompt = "Choose code to apply:", format_item = function(item) return item.language .. " · " .. util.first_line(item.text) end }, function(choice)
      if choice then apply_block(choice) end
    end)
  end
end

function M.setup(options)
  options = vim.tbl_deep_extend("force", defaults, options or {})
  defaults = options
  state.client = Client.new(options)
  state.configured = true
  vim.api.nvim_create_user_command("MedousaToggle", M.toggle, { desc = "Toggle Medousa coding room" })
  vim.api.nvim_create_user_command("MedousaAsk", function() M.ask() end, { desc = "Ask Medousa about the current code" })
  vim.api.nvim_create_user_command("MedousaExplain", M.explain, { range = true, desc = "Explain the current selection with Medousa" })
  vim.api.nvim_create_user_command("MedousaFix", M.fix, { desc = "Ask Medousa to fix current diagnostics" })
  vim.api.nvim_create_user_command("MedousaApply", M.apply, { desc = "Apply a code block from the last Medousa answer" })
  vim.api.nvim_create_user_command("MedousaNew", M.new_session, { desc = "Start a new Medousa conversation" })
  vim.api.nvim_create_user_command("MedousaCancel", M.cancel, { desc = "Stop the active Medousa response" })
  for name, mapping in pairs(options.keymaps or {}) do
    if mapping and mapping ~= false then
      local action = name == "toggle" and M.toggle or name == "ask" and M.ask or name == "explain" and M.explain or name == "fix" and M.fix
      if action then
        local modes = name == "explain" and { "n", "x" } or "n"
        vim.keymap.set(modes, mapping, action, { silent = true, desc = "Medousa " .. name })
      end
    end
  end
  vim.api.nvim_create_user_command("MedousaStatus", function()
    state.client:health(function(_, err) notify(err and "Workshop unavailable" or "Workshop connected") end)
  end, { desc = "Check Medousa workshop connection" })
end

return M
