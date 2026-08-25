# ⚡ md-viewer.nvim

A blazingly fast, lightweight, zero-network-overhead Markdown live preview plugin for Neovim, powered by Rust, Tao, and WebKitGTK.

---

## ✨ Features

- **🚀 Ultra Fast & Lightweight**: Native OS Webview rendering (via `wry`), no bloated Electron, zero idle CPU consumption (0.0%).
- **🔌 Zero Network & Port Conflicts**: Uses pure Unix `STDIN` IPC instead of local HTTP/WebSocket servers.
- **⚡ Real-time Typing Preview**: Preview updates dynamically as you type (unsaved buffer supported) with intelligent 50ms debouncing.
- **🎯 Accurate Sync Scrolling**: Line-level synchronized scrolling based on `comrak` source position mapping.
- **🎨 Pure & Borderless UI**: Clean, distraction-free native dark window perfectly fitted for Tiling Window Managers (Hyprland, Sway, i3, etc.).
- **📦 Zero-Dependency Offline**: Bundled GitHub Dark Markdown stylesheet.

---

## 📋 Requirements

- Linux with `WebKitGTK 4.1` and `GTK 3`:
  - **Arch Linux**: `sudo pacman -S webkit2gtk-4.1 gtk3`
  - **Ubuntu / Debian**: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`
  - **Fedora**: `sudo dnf install webkit2gtk4.1 gtk3`

---

## 📦 Installation

### Using [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
return {
  "Cornfy/md-viewer",
  ft = { "markdown" },
  build = "cargo build --release",
  cmd = { "MdViewerToggle", "MdViewerStart", "MdViewerStop" },
  keys = {
    { "<leader>md", "<cmd>MdViewerToggle<cr>", desc = "Toggle Markdown Preview", ft = "markdown" },
  },
  opts = {
    debounce_ms = 50,
    throttle_ms = 16,
  },
  config = function(_, opts)
    require("md-viewer").setup(opts)
  end,
}
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

MIT License © 2025
