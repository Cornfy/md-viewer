# ⚡ md-viewer.nvim

A blazingly fast, lightweight, zero-network-overhead Markdown live preview plugin for Neovim, powered by Rust, Tao, and WebKitGTK.

 **English** | [简体中文](./README_CN.md) |

---

## ✨ Features

- **🚀 Ultra Fast & Lightweight**: Native OS Webview rendering (via `wry`), no bloated Electron, zero idle CPU consumption (0.0%).
- **🔌 Zero Network & Port Conflicts**: Uses pure Unix `STDIN` IPC instead of local HTTP/WebSocket servers.
- **⚡ Real-time Typing Preview**: Preview updates dynamically as you type (unsaved buffer supported) with intelligent 50ms debouncing.
- **🎯 Precise Range Sync Scrolling**: Line-level synchronized scrolling with sub-line interpolation for long code blocks, tables, and paragraphs.
- **🖼️ Sandboxed Local Image Rendering**: Native support for relative and absolute local images via custom `asset://` protocol with strict 5-layer path isolation.
- **🛡️ Zero-Trust Security**: Enforced Content Security Policy (CSP `script-src 'none'`) blocking all inline scripts and XSS payloads.
- **📑 Multi-Buffer Auto-Sync**: Seamlessly switches document context and working directory when hopping between different Markdown buffers.
- **🎨 Pure & Borderless UI**: Clean, distraction-free native dark window perfectly fitted for Tiling Window Managers (Hyprland, Sway, i3, etc.).
- **📦 Zero-Dependency Offline**: Bundled GitHub Dark Markdown stylesheet.

---

## 📋 Requirements

### 1. Runtime Dependencies (For Normal Users)
End users only need the WebKitGTK runtime libraries:
- **Arch Linux**: `sudo pacman -S webkit2gtk-4.1 gtk3`
- **Ubuntu / Debian**: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`
- **Fedora**: `sudo dnf install webkit2gtk4.1 gtk3`

### 2. Build from Source Dependencies (For Developers)
If you want to compile the binary locally with `cargo build --release`, the development headers and `pkg-config` are required:
- **Arch Linux**: `sudo pacman -S webkit2gtk-4.1 gtk3 pkgconf base-devel`
- **Ubuntu / Debian**: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev pkg-config build-essential`
- **Fedora**: `sudo dnf install webkit2gtk4.1-devel gtk3-devel pkgconf-pkg-config gcc`

---

## 📦 Installation

### Using [lazy.nvim](https://github.com/folke/lazy.nvim)

> **Note**: Pre-compiled binaries will be automatically downloaded from GitHub Releases on first launch!

```lua
return {
  "Cornfy/md-viewer",
  ft = { "markdown" },
  cmd = { "MdViewerToggle", "MdViewerStart", "MdViewerStop" },
  keys = {
    { "<leader>md", "<cmd>MdViewerToggle<cr>", desc = "Toggle Markdown Preview" },
  },
  opts = {
    debounce_ms = 50, -- Debounce rate for typing preview (ms)
    throttle_ms = 16, -- Throttle rate for cursor sync scroll (60fps)
  },
  config = function(_, opts)
    require("md-viewer").setup(opts)
  end,
}
```

---

## ⚙️ Configuration

Default options in `lua/md-viewer/init.lua` :

```lua
require("md-viewer").setup({
  repo = "Cornfy/md-viewer", -- GitHub repository for prebuilt binary releases
  bin_path = nil,            -- Custom executable path (auto-detected if nil)
  debounce_ms = 50,          -- Text synchronization debounce (ms)
  throttle_ms = 16,          -- Scroll synchronization throttle (ms)
})
```

---

## ⌨️ Usage & Commands

| Command / Keymap | Description |
| :--- | :--- |
| `<leader>md` | Toggle Markdown Live Preview |
| `:MdViewerStart` | Start the viewer process |
| `:MdViewerStop` | Stop the viewer process |
| `:MdViewerToggle` | Toggle the viewer process |

---

## 📄 License

MIT License © 2026
