local M = {}

local function trim(value)
  local trimmed = (value or ""):gsub("^%s+", ""):gsub("%s+$", "")
  return trimmed
end

function M.new_parser(on_event)
  local buffer = ""
  local event_name = nil
  local data_lines = nil

  local function dispatch(data)
    local payload = table.concat(data, "\n")
    if payload == "" then
      return
    end
    local ok, value = pcall(vim.json.decode, payload)
    if ok and type(value) == "table" then
      on_event(value, event_name)
    end
    event_name = nil
  end

  return function(chunk)
    buffer = buffer .. (chunk or "")
    while true do
      local newline = buffer:find("\n", 1, true)
      if not newline then
        break
      end
      local line = buffer:sub(1, newline - 1):gsub("\r$", "")
      buffer = buffer:sub(newline + 1)
      if line == "" then
        -- A blank line terminates an SSE event. The payload is assembled below.
      elseif line:sub(1, 6) == "event:" then
        event_name = trim(line:sub(7))
      elseif line:sub(1, 5) == "data:" then
        local data = line:sub(6)
        data_lines = data_lines or {}
        table.insert(data_lines, data:sub(1, 1) == " " and data:sub(2) or data)
      end
      if line == "" and data_lines then
        local data = data_lines
        data_lines = nil
        dispatch(data)
      end
    end
  end
end

function M.extract_code_blocks(markdown)
  local blocks = {}
  local language, body, fence_start = nil, nil, nil
  local line_number = 0
  for line in ((markdown or "") .. "\n"):gmatch("([^\n]*)\n") do
    line_number = line_number + 1
    local fence = line:match("^%s*```([%w_+%-]*)%s*$")
    if fence ~= nil then
      if body then
        table.insert(blocks, {
          language = language,
          text = table.concat(body, "\n"),
          start_line = fence_start,
          end_line = line_number,
        })
        language, body, fence_start = nil, nil, nil
      else
        language, body = fence ~= "" and fence or "text", {}
        fence_start = line_number
      end
    elseif body then
      table.insert(body, line)
    end
  end
  return blocks
end

return M
