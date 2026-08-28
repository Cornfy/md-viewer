local M = {}

M.config = {
  version = "v0.1.1",        -- 当前插件版本号
  repo = "Cornfy/md-viewer", -- GitHub 预编译发布仓库
  bin_path = nil,            -- 自定义二进制路径
  debounce_ms = 50,          -- 打字防抖时长 (ms)
  throttle_ms = 16,          -- 光标滚动节流时长 (ms)
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

-- 获取适合当前系统的 Release 资产文件名与架构标识
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

-- 自动定位二进制路径（支持配置路径、本地下载目录、本地编译目录与全局 PATH）
local function get_or_locate_bin()
  if M.config.bin_path and vim.fn.executable(M.config.bin_path) == 1 then
    return M.config.bin_path
  end

  local plugin_root = get_plugin_root()

  -- 1. 检查自动下载目录 bin/ 及其版本
  local _, bin_name, _ = get_platform_info()
  local downloaded_bin = plugin_root .. "/bin/" .. (bin_name or "md-viewer")
  local version_file = plugin_root .. "/bin/version.txt"

  if vim.fn.executable(downloaded_bin) == 1 then
    local f = io.open(version_file, "r")
    local installed_version = f and f:read("*a") or ""
    if f then f:close() end

    -- 仅当本地版本与当前插件声明版本完全一致时才复用
    if installed_version == M.config.version then
      return downloaded_bin
    end
  end

  -- 2. 检查本地 cargo 编译产物 target/release/
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

-- 异步从 GitHub Releases 静默下载预编译文件
local function download_prebuilt_binary(callback)
  local asset_name, bin_name, ext = get_platform_info()
  if not asset_name then
    vim.notify("[md-viewer] Unsupported OS or architecture. Please build locally with `cargo build --release`", vim.log.levels.ERROR)
    return
  end

  local plugin_root = get_plugin_root()
  local bin_dir = plugin_root .. "/bin"
  vim.fn.mkdir(bin_dir, "p")

  local url = string.format("https://github.com/%s/releases/latest/download/%s", M.config.repo, asset_name)
  local archive_path = bin_dir .. "/" .. asset_name
  local target_bin = bin_dir .. "/" .. bin_name

  vim.notify("[md-viewer] Downloading prebuilt binary (" .. asset_name .. ")...", vim.log.levels.INFO)

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
        -- 记录当前成功下载的版本号
        local vf = io.open(bin_dir .. "/version.txt", "w")
        if vf then
          vf:write(M.config.version)
          vf:close()
        end

        vim.notify("[md-viewer] Prebuilt binary (" .. M.config.version .. ") downloaded successfully!", vim.log.levels.INFO)
        if callback then callback(target_bin) end
      else
        vim.notify("[md-viewer] Download failed. Please check your network connection or proxy settings", vim.log.levels.ERROR)
      end
    end
  })
end

-- 向子进程 STDIN 管道发送 JSON 指令
local function send_ipc(data)
  if job_id then
    local json = vim.json.encode(data)
    vim.fn.chansend(job_id, json .. "\n")
  end
end

-- 同步当前 Buffer 全量文本及所在工作目录
local function sync_content()
  if not job_id then return end
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local content = table.concat(lines, "\n")
  
  -- 未命名新文件的安全回退
  local base_dir = vim.fn.expand("%:p:h")
  if base_dir == "" then
    base_dir = vim.fn.getcwd()
  end

  send_ipc({
    type = "content",
    markdown = content,
    base_dir = base_dir,
  })
end

-- 同步当前光标所在行号
local function sync_scroll()
  if not job_id then return end
  local line = vim.fn.line(".")
  send_ipc({ type = "scroll", line = line })
end

-- 文本防抖同步
local function debounced_sync_content()
  content_timer:stop()
  content_timer:start(M.config.debounce_ms, 0, vim.schedule_wrap(sync_content))
end

-- 滚动节流同步
local function throttled_sync_scroll()
  scroll_timer:stop()
  scroll_timer:start(M.config.throttle_ms, 0, vim.schedule_wrap(sync_scroll))
end

-- 启动并接管 Viewer 进程
local function spawn_viewer(bin)
  job_id = vim.fn.jobstart({ bin, "-" }, {
    on_exit = function()
      M.stop()
    end,
  })

  if job_id <= 0 then
    vim.notify("[md-viewer] Failed to spawn process: " .. bin, vim.log.levels.ERROR)
    job_id = nil
    return
  end

  augroup = vim.api.nvim_create_augroup("MdViewerSyncGroup", { clear = true })

  -- 监听打字与切换 Buffer 事件
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI", "BufEnter" }, {
    group = augroup,
    pattern = { "*.md", "*.markdown" },
    callback = function()
      debounced_sync_content()
      throttled_sync_scroll()
    end,
  })

  -- 监听光标移动事件
  vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI" }, {
    group = augroup,
    pattern = { "*.md", "*.markdown" },
    callback = throttled_sync_scroll,
  })

  -- 退出 Neovim 时自动杀死子进程
  vim.api.nvim_create_autocmd("VimLeavePre", {
    group = augroup,
    callback = M.stop,
  })

  -- 延迟 100ms 发送首屏同步数据
  vim.defer_fn(function()
    sync_content()
    sync_scroll()
  end, 100)

  vim.notify("[md-viewer] Live preview started", vim.log.levels.INFO)
end

-- 启动预览
function M.start()
  if job_id then return end

  local bin = get_or_locate_bin()
  if bin then
    spawn_viewer(bin)
  else
    download_prebuilt_binary(function(downloaded_bin)
      spawn_viewer(downloaded_bin)
    end)
  end
end

-- 停止并清理资源
function M.stop()
  if content_timer then content_timer:stop() end
  if scroll_timer then scroll_timer:stop() end

  if job_id then
    vim.fn.jobstop(job_id)
    job_id = nil
  end

  if augroup then
    vim.api.nvim_del_augroup_by_id(augroup)
    augroup = nil
  end
end

-- 开关切换
function M.toggle()
  if job_id then
    M.stop()
  else
    M.start()
  end
end

-- 初始化插件配置与用户指令
function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})

  vim.api.nvim_create_user_command("MdViewerStart", M.start, {})
  vim.api.nvim_create_user_command("MdViewerStop", M.stop, {})
  vim.api.nvim_create_user_command("MdViewerToggle", M.toggle, {})
end

return M
