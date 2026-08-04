local M = {}

local function label(item)
  if item.label then return item.label end
  local preview = item.preview ~= "" and " · " .. item.preview or ""
  return item.display_name .. preview
end

local function telescope_select(items, prompt, callback)
  local ok_pickers, pickers = pcall(require, "telescope.pickers")
  local ok_finders, finders = pcall(require, "telescope.finders")
  local ok_config, config = pcall(require, "telescope.config")
  local ok_actions, actions = pcall(require, "telescope.actions")
  local ok_state, action_state = pcall(require, "telescope.actions.state")
  if not (ok_pickers and ok_finders and ok_config and ok_actions and ok_state) then return false end

  pickers.new({}, {
    prompt_title = prompt,
    finder = finders.new_table({
      results = items,
      entry_maker = function(item)
        return { value = item, display = label(item), ordinal = label(item) }
      end,
    }),
    sorter = config.values.generic_sorter({}),
    attach_mappings = function(prompt_bufnr)
      actions.select_default:replace(function()
        local selection = action_state.get_selected_entry()
        actions.close(prompt_bufnr)
        if selection then callback(selection.value) end
      end)
      return true
    end,
  }):find()
  return true
end

function M.sessions(items, callback)
  if telescope_select(items, "Medousa conversations", callback) then return end
  vim.ui.select(items, {
    prompt = "Medousa conversations",
    format_item = label,
  }, callback)
end

function M.modes(items, callback)
  if telescope_select(items, "Medousa mode", callback) then return end
  vim.ui.select(items, {
    prompt = "How should Medousa work in this conversation?",
    format_item = label,
  }, callback)
end

function M.projects(items, callback)
  if telescope_select(items, "Medousa projects", callback) then return end
  vim.ui.select(items, {
    prompt = "Choose the governed project for this conversation",
    format_item = label,
  }, callback)
end

return M
