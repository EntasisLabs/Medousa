local M = {}

function M.trim(value)
  return (value or ""):gsub("^%s+", ""):gsub("%s+$", "")
end

function M.first_line(value)
  local line = (value or ""):match("([^\n]+)") or ""
  line = M.trim(line)
  if #line > 72 then
    return line:sub(1, 71) .. "…"
  end
  return line
end

function M.session_path()
  return vim.fn.stdpath("state") .. "/medousa-session.json"
end

function M.read_session()
  local path = M.session_path()
  local file = io.open(path, "r")
  if not file then
    return nil
  end
  local content = file:read("*a")
  file:close()
  local ok, value = pcall(vim.json.decode, content)
  if ok and type(value) == "table" and type(value.session_id) == "string" then
    return value
  end
  return nil
end

function M.write_session(session_id)
  local path = M.session_path()
  vim.fn.mkdir(vim.fn.fnamemodify(path, ":h"), "p")
  local file = assert(io.open(path, "w"))
  file:write(vim.json.encode({ session_id = session_id }))
  file:close()
end

function M.shell_escape(value)
  return vim.fn.shellescape(value or "")
end

return M
