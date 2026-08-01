local stream = require("medousa.stream")
local util = require("medousa.util")

local M = {}
M.__index = M

local function url_encode(value)
  local encoded = tostring(value):gsub("([^%w%-_%.~])", function(char)
    return string.format("%%%02X", string.byte(char))
  end)
  return encoded
end

local function resolve_url(endpoint, path)
  if path:match("^https?://") then
    return path
  end
  return endpoint .. path
end

local function response_body(result)
  local output = result.stdout or ""
  local body, status = output:match("^(.*)\n(%d%d%d)\n?$")
  return body or output, tonumber(status)
end

local function decode_body(body)
  local ok, value = pcall(vim.json.decode, body)
  if not ok then
    return nil, "Medousa returned invalid JSON: " .. body:sub(1, 240)
  end
  return value
end

function M.new(options)
  local endpoint = (options.endpoint or "http://127.0.0.1:7419"):gsub("/$", "")
  return setmetatable({
    endpoint = endpoint,
    token = options.token or vim.env.MEDOUSA_TOKEN,
    stream_job = nil,
    stream_cancelled = false,
  }, M)
end

function M:headers()
  local headers = { "-H", "Accept: application/json" }
  if self.token and self.token ~= "" then
    table.insert(headers, "-H")
    table.insert(headers, "Authorization: Bearer " .. self.token)
  end
  return headers
end

function M:request(method, path, body, callback)
  local args = { "curl", "-sS", "--fail-with-body", "-w", "\n%{http_code}", "-X", method }
  vim.list_extend(args, self:headers())
  if body then
    table.insert(args, "-H")
    table.insert(args, "Content-Type: application/json")
    table.insert(args, "--data-binary")
    table.insert(args, vim.json.encode(body))
  end
  table.insert(args, resolve_url(self.endpoint, path))
  vim.system(args, { text = true }, function(result)
    vim.schedule(function()
      local response, status = response_body(result)
      if result.code ~= 0 then
        callback(nil, (result.stderr and result.stderr ~= "" and result.stderr) or response or "Medousa request failed")
        return
      end
      if status and status >= 400 then
        callback(nil, "Medousa request failed (HTTP " .. status .. ")")
        return
      end
      if status == 204 or response == "" then
        callback({}, nil)
        return
      end
      local value, err = decode_body(response)
      callback(value, err)
    end)
  end)
end

function M:health(callback)
  self:request("GET", "/health", nil, callback)
end

function M:history(session_id, callback)
  self:request("GET", "/v1/sessions/" .. url_encode(session_id) .. "/history", nil, callback)
end

function M:sessions(limit, callback)
  self:request("GET", "/v1/sessions?limit=" .. tostring(limit or 50), nil, function(value, err)
    if not value then
      callback(nil, err)
    elseif vim.islist(value) then
      callback(value, nil)
    else
      callback(value.sessions or {}, nil)
    end
  end)
end

function M:rename_session(session_id, display_name, callback)
  self:request("PUT", "/v1/sessions/" .. url_encode(session_id) .. "/name", {
    display_name = display_name,
  }, callback)
end

function M:delete_session(session_id, callback)
  self:request("DELETE", "/v1/sessions/" .. url_encode(session_id) .. "?purge_memory=true", nil, callback)
end

function M:create_session(callback)
  self:request("POST", "/v1/sessions", { catalog = "single" }, callback)
end

function M:ensure_session(callback)
  local stored = util.read_session()
  if stored then
    self:history(stored.session_id, function(history, err)
      if history then
        callback(stored.session_id, history, nil)
      elseif err and err:find("404", 1, true) then
        self:create_session(function(created, create_err)
          if not created then
            callback(nil, nil, create_err)
            return
          end
          util.write_session(created.session_id)
          callback(created.session_id, { session_id = created.session_id, turns = {} }, nil)
        end)
      else
        callback(nil, nil, err)
      end
    end)
    return
  end
  self:create_session(function(created, err)
    if not created then
      callback(nil, nil, err)
      return
    end
    util.write_session(created.session_id)
    callback(created.session_id, { session_id = created.session_id, turns = {} }, nil)
  end)
end

function M:cancel(session_id, callback)
  self.stream_cancelled = true
  if self.stream_job then
    self.stream_job:kill(15)
    self.stream_job = nil
  end
  self:request("POST", "/v1/sessions/" .. url_encode(session_id) .. "/active-turn", { cancel = true }, function(_, err)
    if callback then callback(err) end
  end)
end

function M:approve_budget(request_id, extra_rounds, callback)
  self:request("POST", "/v1/turns/budget-requests/" .. url_encode(request_id) .. "/approve", {
    extra_rounds = extra_rounds,
    resolved_by = "neovim",
  }, callback)
end

function M:deny_budget(request_id, callback)
  self:request("POST", "/v1/turns/budget-requests/" .. url_encode(request_id) .. "/deny", {
    resolved_by = "neovim",
  }, callback)
end

function M:resolve_permission(request_id, approve, callback)
  local action = approve and "approve" or "deny"
  self:request("POST", "/v1/agents/permission-requests/" .. url_encode(request_id) .. "/" .. action, {
    resolved_by = "neovim",
  }, callback)
end

function M:turn(session_id, prompt, context, callbacks)
  self.stream_cancelled = false
  self.stream_terminal = false
  self:request("GET", "/v1/runtime/defaults", nil, function(defaults, defaults_err)
    if not defaults then
      callbacks.on_error(defaults_err)
      return
    end
    local request = {
      model = defaults.model,
      persist_user_turn = true,
      prompt = prompt .. "\n\n" .. context,
      provider = defaults.provider,
      response_depth_mode = defaults.response_depth_mode,
      reasoning_effort = defaults.reasoning_effort,
      session_id = session_id,
      stage_routing = defaults.stage_routing,
      media_refs = {},
      surface = {
        channel_surface = "neovim",
        supports_browser_host = false,
        supports_ui_artifacts = false,
      },
    }
    self:request("POST", "/v1/interactive/turn", request, function(turn, turn_err)
      if not turn then
        callbacks.on_error(turn_err)
        return
      end
      self:_stream(turn.stream_url, 0, 0, callbacks)
    end)
  end)
end

function M:_stream(stream_url, since, attempt, callbacks)
  if self.stream_cancelled then return end
  local terminal_seen = false
  local parser = stream.new_parser(function(event)
    local sequence = tonumber(event.seq)
    if sequence and sequence > since then since = sequence end
    local is_terminal = event.terminal and not self.stream_terminal
    if is_terminal then
      terminal_seen = true
      self.stream_terminal = true
    end
    vim.schedule(function()
      callbacks.on_event(event)
      if is_terminal then callbacks.on_done(event) end
    end)
  end)
  local args = { "curl", "-sS", "-N", "--no-buffer" }
  vim.list_extend(args, self:headers())
  local separator = stream_url:find("?", 1, true) and "&" or "?"
  table.insert(args, resolve_url(self.endpoint, stream_url) .. separator .. "since=" .. tostring(since))
  self.stream_job = vim.system(args, {
    text = true,
    stdout = function(_, data)
      if data then parser(data) end
    end,
  }, function(result)
    vim.schedule(function()
      self.stream_job = nil
      if self.stream_cancelled then return end
      if self.stream_terminal or terminal_seen then return end
      if attempt < 4 then
        callbacks.on_status("Connection interrupted — recovering…")
        vim.defer_fn(function()
          self:_stream(stream_url, since, attempt + 1, callbacks)
        end, math.min(30000, 500 * (2 ^ attempt)))
      else
        callbacks.on_error((result.stderr and result.stderr ~= "" and result.stderr) or "Medousa stream ended unexpectedly")
      end
    end)
  end)
end

return M
