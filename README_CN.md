# ⚡ md-viewer.nvim

极速、轻量、零网络开销的 Neovim Markdown 实时预览插件，基于 Rust、Tao 和 WebKitGTK 构建。

[English](./README.md) | **简体中文**

---

## ✨ 核心特性

- **🚀 极致性能与轻量化**：基于系统原生 Webview（`wry`）渲染，告别臃肿的 Electron，待机状态下 **0.0% CPU 占用**。
- **🔌 零网络开销与端口冲突**：采用纯 Unix `STDIN` 进程管道通信，**不监听任何本地 HTTP/WebSocket 端口**，多开实例永不冲突。
- **⚡ 打字实时预览**：在 Insert 模式下打字即时更新渲染（支持未保存的内存 Buffer），内置 50ms 智能防抖。
- **🎯 精准区间同步滚动**：基于 AST 行号范围映射，支持长代码块、大表格和多行段落内的**比例平滑插值滚动**与空行智能回退。
- **🖼️ 沙箱级本地图片渲染**：通过自定义 `asset://` 虚拟协议支持相对/绝对路径本地图片，内置严苛的**五重路径安全沙箱**，防止私钥等敏感文件越狱泄露。
- **🛡️ 零信任安全防护**：内置严苛的内容安全策略（CSP `script-src 'none'`），彻底封杀任何内嵌恶意脚本与 XSS 渗透载荷。
- **📑 多 Buffer 自动感知**：在多个 Markdown 标签页之间切换（`BufEnter`）时，自动无缝同步最新内容与图片上下文目录。
- **🎨 极简无边框沉浸 UI**：专为平铺式窗口管理器（Hyprland、Sway、i3 等）量身打造的纯粹深色窗口。
- **📦 离线零外部依赖**：内嵌 GitHub Dark 样式表，无网/弱网环境下启动即秒开。

---

## 📋 系统依赖

### 1. 运行时依赖（普通用户）
普通用户无需安装 Rust 工具链，首次启动将自动从 GitHub Releases 下载预编译文件。只需确保系统具备基础的 WebKitGTK 图形库：
- **Arch Linux**: `sudo pacman -S webkit2gtk-4.1 gtk3`
- **Ubuntu / Debian**: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`
- **Fedora**: `sudo dnf install webkit2gtk4.1 gtk3`

### 2. 源码编译依赖（开发者 / 本地编译）
若希望在本地通过 `cargo build --release` 编译二进制，需安装开发头文件与 `pkg-config`：
- **Arch Linux**: `sudo pacman -S webkit2gtk-4.1 gtk3 pkgconf base-devel`
- **Ubuntu / Debian**: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev pkg-config build-essential`
- **Fedora**: `sudo dnf install webkit2gtk4.1-devel gtk3-devel pkgconf-pkg-config gcc`

---

## 📦 安装指南

### 使用 [lazy.nvim](https://github.com/folke/lazy.nvim)

> **说明**：首次按下 `<leader>md` 时，插件会自动从 GitHub Releases 静默下载适合当前系统的预编译二进制文件。

```lua
return {
  "Cornfy/md-viewer",
  ft = { "markdown" },
  cmd = { "MdViewerToggle", "MdViewerStart", "MdViewerStop" },
  keys = {
    { "<leader>md", "<cmd>MdViewerToggle<cr>", desc = "Toggle Markdown Preview" },
  },
  opts = {
    debounce_ms = 50, -- 输入打字内容同步防抖时间 (毫秒)
    throttle_ms = 16, -- 光标滚动同步节流时间 (约 60fps)
  },
  config = function(_, opts)
    require("md-viewer").setup(opts)
  end,
}
```

---

## ⚙️ 详细配置项

`lua/md-viewer/init.lua` 中的默认配置如下：

```lua
require("md-viewer").setup({
  repo = "Cornfy/md-viewer", -- GitHub 预编译 Release 仓库地址
  bin_path = nil,            -- 自定义可执行文件路径 (为 nil 时自动探测)
  debounce_ms = 50,          -- 文本同步防抖时长 (ms)
  throttle_ms = 16,          -- 滚动同步节流时长 (ms)
})
```

---

## ⌨️ 快捷键与指令

| 快捷键 / 命令 | 功能说明 |
| :--- | :--- |
| `<leader>md` | 开关 Markdown 实时预览窗口 (Toggle) |
| `:MdViewerStart` | 启动预览器进程 |
| `:MdViewerStop` | 关闭预览器进程 |
| `:MdViewerToggle` | 切换预览器开启/关闭状态 |

---

## 📄 开源协议

[MIT License](./LICENSE) © 2026
