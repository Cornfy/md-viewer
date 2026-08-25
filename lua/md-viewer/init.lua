local M = {}

M.config = {
  repo = "Cornfy/md-viewer", -- GitHub 仓库
  bin_path = nil,
  debounce_ms = 50,
  throttle_ms = 16,
}

local job_id = nil
local augroup = nil
local uv = vim.uv or vim.loop
local content_timer = uv.new_timer()
local scroll_timer = uv.new_timer()

-- 获取当前插件根目录
local function get_plugin_root()
  local script_path = debug.getinfo(1, "S").source:sub(2)
  return vim.fn.fnamemodify(script_path, ":p:h:h:h")
end

-- 获取适合当前系统的 Release 文件名与可执行文件路径
local function get_platform_info()
  local os_name = uv.os_uname().sysname
  local arch = uv.os_uname().machine

  local target = nil
  local ext = "tar.gz"
  local bin_name = "md-viewer"

  if os_name == "Linux" then
    if arch == "x86_64" then
      target = "x86_64-unknown-linux-gnu"
    elseif arch == "aarch64" or arch == "arm64" then
      target = "aarch64-unknown-linux-gnu"
    end
  elseif os_name == "Darwin" then -- macOS
    if arch == "arm64" or arch == "aarch64" then
      target = "aarch64-apple-darwin"
    else
      target = "x86_64-apple-darwin"
    end
  elseif os_name == "Windows_NT" then
    target = "x86_64-pc-windows-msvc"
    ext = "zip"
    bin_name = "md-viewer.exe"
  end

  if not target then
    return nil, nil, nil
  end

  local asset_name = string.format("md-viewer-%s.%s", target, ext)
  return asset_name, bin_name, ext
end

-- 自动定位二进制；若不存在则返回可下载的路径
local function get_or_locate_bin()
  if M.config.bin_path and vim.fn.executable(M.config.bin_path) == 1 then
    return M.config.bin_path
  end

  local plugin_root = get_plugin_root()

  -- 1. 检查插件目录自带的 bin/md-viewer (自动下载目录)
  local _, bin_name, _ = get_platform_info()
  local downloaded_bin = plugin_root .. "/bin/" .. (bin_name or "md-viewer")
  if vim.fn.executable(downloaded_bin) == 1 then
    return downloaded_bin
  end

  -- 2. 检查本地 cargo build target 产物
  local local_target_bin = plugin_root .. "/target/release/" .. (bin_name or "md-viewer")
  if vim.fn.executable(local_target_bin) == 1 then
    return local_target_bin
  end

  -- 3. 检查系统全局 PATH
  if vim.fn.executable("md-viewer") == 1 then
    return "md-viewer"
  end

  return nil
end

-- 异步从 GitHub Releases 下载预编译文件
local function download_prebuilt_binary(callback)
  local asset_name, bin_name, ext = get_platform_info()
  if not asset_name then
    vim.notify("[md-viewer] 无法识别当前操作系统架构，请尝试本地运行 `cargo build --release`", vim.log.levels.ERROR)
    return
  end

  local plugin_root = get_plugin_root()
  local bin_dir = plugin_root .. "/bin"
  vim.fn.mkdir(bin_dir, "p")

  local url = string.format("https://github.com/%s/releases/latest/download/%s", M.config.repo, asset_name)
  local archive_path = bin_dir .. "/" .. asset_name
  local target_bin = bin_dir .. "/" .. bin_name

  vim.notify("[md-viewer] 正在下载预编译文件 (" .. asset_name .. ")...", vim.log.levels.INFO)

  -- 构建下载并解压的 Shell 命令
  local cmd
  if ext == "tar.gz" then
    cmd = string.format("curl -sL '%s' -o '%s' && tar -xzf '%s' -C '%s' && chmod +x '%s' && rm '%s'",
      url, archive_path, archive_path, bin_dir, target_bin, archive_path)
  else
    cmd = string.format("curl -sL '%s' -o '%s' && unzip -o '%s' -d '%s' && rm '%s'",
      url, archive_path, archive_path, bin_dir, archive_path)
  end

  vim.fn.jobstart({ "sh", "-c", cmd }, {
    on_exit = function(_, exit_code)
      if exit_code == 0 and vim.fn.executable(target_bin) == 1 then
        vim.notify("[md-viewer] 预编译文件下载成功！", vim.log.levels.INFO)
        if callback then callback(target_bin) end
      else
        vim.notify("[md-viewer] 下载失败，请检查网络或配置代理", vim.log.levels.ERROR)
      end
    end
  })
end

local function send_ipc(data)
  if job_id then
    local json = vim.json.encode(data)
    vim.fn.chansend(job_id, json .. "\n")
  end
end

local function sync_content()
  if not job_id then return end
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local content = table.concat(lines, "\n")
  send_ipc({ type = "content", markdown = content })
end

local function sync_scroll()
  if not job_id then return end
  local line = vim.fn.line(".")
  send_ipc({ type = "scroll", line = line })
end

local function debounced_sync_content()
  content_timer:stop()
  content_timer:start(M.config.debounce_ms, 0, vim.schedule_wrap(sync_content))
end

local function throttled_sync_scroll()
  scroll_timer:stop()
  scroll_timer:start(M.config.throttle_ms, 0, vim.schedule_wrap(sync_scroll))
end

local function spawn_viewer(bin)
  job_id = vim.fn.jobstart({ bin, "-" }, {
    on_exit = function()
      M.stop()
    end,
  })

  if job_id <= 0 then
    vim.notify("[md-viewer] 启动进程失败: " .. bin, vim.log.levels.ERROR)
    job_id = nil
    return
  end

  augroup = vim.api.nvim_create_augroup("MdViewerSyncGroup", { clear = true })

  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    group = augroup,
    pattern = "*.md",
    callback = debounced_sync_content,
  })

  vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI" }, {
    group = augroup,
    pattern = "*.md",
    callback = throttled_sync_scroll,
  })

  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = augroup,
    callback = M.stop,
  })

  vim.defer_fn(function()
    sync_content()
    sync_scroll()
  end, 100)

  vim.notify("[md-viewer] 实时预览已开启", vim.log.levels.INFO)
end

function M.start()
  if job_id then return end

  local bin = get_or_locate_bin()
  if bin then
    spawn_viewer(bin)
  else
    -- 如果本地没有，自动静默下载 Release
    download_prebuilt_binary(function(downloaded_bin)
      spawn_viewer(downloaded_bin)
    end)
  end
end

function M.stop()
  if job_id then
    vim.fn.jobstop(job_id)
    job_id = nil
  end

  if augroup then
    vim.api.nvim_del_augroup_by_id(augroup)
    augroup = nil
  end
end

function M.toggle()
  if job_id then
    M.stop()
  else
    M.start()
  end
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})

  vim.api.nvim_create_user_command("MdViewerStart", M.start, {})
  vim.api.nvim_create_user_command("MdViewerStop", M.stop, {})
  vim.api.nvim_create_user_command("MdViewerToggle", M.toggle, {})
end

return M
