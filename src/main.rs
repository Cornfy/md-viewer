use comrak::{markdown_to_html, ComrakOptions};
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use std::{
    env, fs,
    io::{self, BufRead},
    path::{Component, Path, PathBuf},
    sync::{mpsc, Arc, RwLock},
    thread,
    time::Duration,
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

#[cfg(target_os = "linux")]
use gtk::prelude::*;
#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

// 1. IPC 进程间通信协议结构定义
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcMessage {
    #[serde(rename = "content")]
    Content {
        markdown: String,
        base_dir: Option<String>,
    },
    #[serde(rename = "scroll")]
    Scroll { line: usize },
}

// 2. 主线程 GUI 事件驱动定义
#[derive(Debug)]
enum AppEvent {
    UpdateHtml { html: String, base_dir: PathBuf },
    ScrollTo(usize),
    ReloadFile(PathBuf),
}

// 离线内嵌 GitHub 样式表
const CSS_STYLE: &str = include_str!("../assets/github-markdown-dark.min.css");

// ==========================================
// 🛡️ 安全门禁系统 (Security Shield)
// ==========================================

/// 图片 MIME 类型映射表
fn get_image_mime(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}

/// 纯标准库 URL 百分比解码（支持中文与空格路径）
fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                if let Ok(val) = u8::from_str_radix(std::str::from_utf8(&[h1, h2]).unwrap_or("0"), 16) {
                    bytes.push(val);
                    continue;
                }
            }
        }
        bytes.push(b);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

/// 核心沙箱安全检查：防止路径穿越 (Path Traversal) 与敏感文件探测
fn is_safe_asset_path(requested_path: &Path, base_dir: &Path) -> Option<PathBuf> {
    // 1. 规范化物理路径（展开软链接和 ..）
    let canonical_target = requested_path.canonicalize().ok()?;
    let canonical_base = base_dir.canonicalize().ok()?;

    // 2. 严禁访问系统核心敏感大目录
    let prohibited_roots = ["/etc", "/proc", "/sys", "/dev", "/run", "/var", "/root"];
    for bad in prohibited_roots {
        if canonical_target.starts_with(bad) {
            return None;
        }
    }

    // 3. 严禁访问任何隐藏文件或隐藏目录 (.git, .ssh, .env 等)
    for component in canonical_target.components() {
        if let Component::Normal(name) = component {
            if name.to_string_lossy().starts_with('.') {
                return None;
            }
        }
    }

    // 4. 扩展名白名单校验（只允许安全媒体资源）
    let ext = canonical_target.extension()?.to_string_lossy();
    if get_image_mime(&ext).is_none() {
        return None;
    }

    // 5. 必须严格在 base_dir 沙箱内部
    if !canonical_target.starts_with(&canonical_base) {
        return None;
    }

    Some(canonical_target)
}

/// 校验是否为合法的本地 Markdown 互链目标（用于新开窗口预览）
fn resolve_safe_markdown_target(target_path: &Path, base_dir: &Path) -> Option<PathBuf> {
    let canonical_target = target_path.canonicalize().ok()?;
    let canonical_base = base_dir.canonicalize().ok()?;

    // 必须在 base_dir 沙箱内部
    if !canonical_target.starts_with(&canonical_base) {
        return None;
    }

    // 严格限制扩展名必须为 Markdown
    let ext = canonical_target.extension()?.to_string_lossy().to_lowercase();
    if ext != "md" && ext != "markdown" {
        return None;
    }

    // 严禁隐藏文件
    for component in canonical_target.components() {
        if let Component::Normal(name) = component {
            if name.to_string_lossy().starts_with('.') {
                return None;
            }
        }
    }

    Some(canonical_target)
}

// ==========================================
// ⚙️ Markdown 解析与 HTML 外壳构建
// ==========================================

fn render_markdown(content: &str) -> String {
    let mut options = ComrakOptions::default();
    options.render.sourcepos = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    markdown_to_html(content, &options)
}

// 注入到 Webview 底层的初始化核心脚本（宿主受信任环境执行）
const INIT_SCRIPT: &str = r#"
window.updateContent = (html, baseDir) => {
  let baseTag = document.querySelector("base");
  if (!baseTag) {
    baseTag = document.createElement("base");
    document.head.appendChild(baseTag);
  }
  if (baseDir) {
    baseTag.href = "asset://localhost" + (baseDir.endsWith("/") ? baseDir : baseDir + "/");
  }
  const container = document.getElementById("content");
  if (container) {
    container.innerHTML = html;
  }
};

window.scrollToLine = (line) => {
  const elements = Array.from(document.querySelectorAll("[data-sourcepos]"));
  if (elements.length === 0) return;

  let bestMatch = null;
  let minSpan = Infinity;
  let prevElement = null;

  for (const el of elements) {
    const pos = el.getAttribute("data-sourcepos");
    const match = pos.match(/^(\d+):\d+-(\d+):\d+$/);
    if (!match) continue;

    const startLine = parseInt(match[1], 10);
    const endLine = parseInt(match[2], 10);

    // 1. 精准区间命中
    if (line >= startLine && line <= endLine) {
      const span = endLine - startLine;
      if (span < minSpan) {
        minSpan = span;
        bestMatch = { el, startLine, endLine };
      }
    }

    // 2. 记录光标前最近的元素以供空行回退
    if (endLine < line) {
      prevElement = el;
    }
  }

  if (bestMatch) {
    const { el, startLine, endLine } = bestMatch;
    // 多行大代码块/大段落：按比例平滑插值滚动
    if (endLine > startLine + 2) {
      const progress = (line - startLine) / (endLine - startLine);
      const rect = el.getBoundingClientRect();
      const targetY = window.scrollY + rect.top + (rect.height * progress) - (window.innerHeight / 2);
      window.scrollTo({ top: targetY, behavior: "smooth" });
    } else {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  } else if (prevElement) {
    // 3. 空行回退定位
    prevElement.scrollIntoView({ behavior: "smooth", block: "center" });
  }
};
"#;

fn generate_html_shell(initial_html: &str, initial_base_dir: Option<&Path>) -> String {
    let base_tag = if let Some(dir) = initial_base_dir {
        let mut dir_str = dir.to_string_lossy().into_owned();
        if !dir_str.ends_with('/') {
            dir_str.push('/');
        }
        format!(r#"<base href="asset://localhost{}">"#, dir_str)
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <!-- 零信任 CSP 策略：禁止一切外部脚本与 XSS 执行 -->
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src asset: https: http: data:;">
  {base_tag}
  <style>
    {CSS_STYLE}
    body {{
      box-sizing: border-box;
      min-width: 200px;
      max-width: 100%;
      margin: 0 auto;
      padding: 30px 45px;
      background-color: #0d1117;
    }}
    html {{
      scroll-behavior: smooth;
    }}
    pre, table {{
      overflow-x: auto;
    }}
  </style>
</head>
<body class="markdown-body">
  <div id="content">{initial_html}</div>
</body>
</html>"#
    )
}

// ==========================================
// 🚀 程序主入口
// ==========================================

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:\n  md-viewer <file.md>  (Watch file mode)\n  md-viewer -          (Listen to STDIN, for Neovim)");
        std::process::exit(1);
    }

    let input_mode = args[1].clone();
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 线程安全的当前活动目录沙箱状态
    let active_base_dir = Arc::new(RwLock::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))));

    let initial_html: String;
    let initial_base_dir: Option<PathBuf>;

    if input_mode == "-" {
        // 模式 A：STDIN 模式（配合 Neovim 实时通信）
        initial_html = String::new();
        initial_base_dir = None;

        let proxy_clone = proxy.clone();
        let base_dir_lock = active_base_dir.clone();
        thread::spawn(move || {
            let stdin = io::stdin();
            let handle = stdin.lock();
            for line in handle.lines().flatten() {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&line) {
                    match msg {
                        IpcMessage::Content { markdown, base_dir } => {
                            let dir = base_dir
                                .map(PathBuf::from)
                                .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

                            if let Ok(mut lock) = base_dir_lock.write() {
                                *lock = dir.clone();
                            }

                            let html = render_markdown(&markdown);
                            let _ = proxy_clone.send_event(AppEvent::UpdateHtml { html, base_dir: dir });
                        }
                        IpcMessage::Scroll { line } => {
                            let _ = proxy_clone.send_event(AppEvent::ScrollTo(line));
                        }
                    }
                }
            }
            // 管道断开时自动平稳退出
            std::process::exit(0);
        });
    } else {
        // 模式 B：本地独立文件监听模式
        let file_path = PathBuf::from(&input_mode).canonicalize().expect("Invalid file path");
        let parent_dir = file_path.parent().unwrap_or_else(|| Path::new("/")).to_owned();

        if let Ok(mut lock) = active_base_dir.write() {
            *lock = parent_dir.clone();
        }

        // 首屏同步渲染，消灭启动白屏
        let content = fs::read_to_string(&file_path).unwrap_or_default();
        initial_html = render_markdown(&content);
        initial_base_dir = Some(parent_dir.clone());

        let proxy_clone = proxy.clone();
        let target_filename = file_path.file_name().unwrap().to_owned();

        thread::spawn(move || {
            let (tx, rx) = mpsc::channel();
            let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                if let Ok(event) = res {
                    if event.paths.iter().any(|p| p.file_name() == Some(&target_filename)) {
                        let _ = tx.send(());
                    }
                }
            }).unwrap();

            watcher.watch(&parent_dir, RecursiveMode::NonRecursive).unwrap();

            loop {
                if rx.recv().is_ok() {
                    // 50ms 事件防抖队列
                    thread::sleep(Duration::from_millis(50));
                    while rx.try_recv().is_ok() {}
                    let _ = proxy_clone.send_event(AppEvent::ReloadFile(file_path.clone()));
                }
            }
        });
    }

    // 创建无边框沉浸式窗口
    let window = WindowBuilder::new()
        .with_title("Native Markdown Viewer")
        .with_decorations(false)
        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
        .build(&event_loop)
        .unwrap();

    let base_dir_for_protocol = active_base_dir.clone();
    let base_dir_for_nav = active_base_dir.clone();

    let builder = WebViewBuilder::new()
        // 1. 注册 asset:// 虚拟图片协议
        .with_custom_protocol("asset".into(), move |_id, request| {
            let uri = request.uri().path();
            let decoded_path = percent_decode(uri);
            let target_path = PathBuf::from(&decoded_path);

            let current_base = base_dir_for_protocol.read().unwrap().clone();

            if let Some(safe_path) = is_safe_asset_path(&target_path, &current_base) {
                if let Ok(data) = fs::read(&safe_path) {
                    let ext = safe_path.extension().unwrap_or_default().to_string_lossy();
                    let mime = get_image_mime(&ext).unwrap_or("application/octet-stream");
                    return wry::http::Response::builder()
                        .header("Content-Type", mime)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(data.into())
                        .unwrap();
                }
            }

            // 拒绝越权访问
            wry::http::Response::builder()
                .status(wry::http::StatusCode::FORBIDDEN)
                .body(b"403 Forbidden: Sandbox access denied".as_slice().into())
                .unwrap()
        })
        // 2. 注入受信任的初始化通信脚本
        .with_initialization_script(INIT_SCRIPT)
        // 3. 智能超链接导航拦截器 (修复首屏被误杀的 Bug)
        .with_navigation_handler(move |url| {
            // A. 外部网页链接：调用默认浏览器打开，拦截 Webview 内部跳转
            if url.starts_with("http://") || url.starts_with("https://") {
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&url).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
                return false;
            }

            // B. 本地 Markdown 互链跳转 (.md / .markdown)：校验通过后拉起新窗口预览
            if url.contains(".md") || url.contains(".markdown") {
                let clean_path_str = url
                    .trim_start_matches("asset://localhost")
                    .trim_start_matches("file://");
                let decoded = percent_decode(clean_path_str);
                let target_path = PathBuf::from(&decoded);
                let current_base = base_dir_for_nav.read().unwrap().clone();

                if let Some(safe_md_file) = resolve_safe_markdown_target(&target_path, &current_base) {
                    if let Ok(current_exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(current_exe)
                            .arg(safe_md_file)
                            .spawn();
                    }
                }
                return false; // 拦截！不让当前窗口自身跳转
            }

            // C. 允许首屏初始化、基础页面加载、图片协议与页内锚点放行！
            true
        })
        .with_html(generate_html_shell(&initial_html, initial_base_dir.as_deref()));

    #[cfg(target_os = "linux")]
    let webview = {
        let gtk_window = window.gtk_window();
        let vbox = gtk_window
            .child()
            .expect("Window has no child container")
            .downcast::<gtk::Box>()
            .expect("Window child container is not a GtkBox");
        builder.build_gtk(&vbox).unwrap()
    };

    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window).unwrap();

    // 事件循环处理
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(app_event) => match app_event {
                AppEvent::UpdateHtml { html, base_dir } => {
                    let dir_str = base_dir.to_string_lossy();
                    if let (Ok(js_html), Ok(js_dir)) = (serde_json::to_string(&html), serde_json::to_string(&dir_str)) {
                        let script = format!("window.updateContent({}, {});", js_html, js_dir);
                        let _ = webview.evaluate_script(&script);
                    }
                }
                AppEvent::ScrollTo(line) => {
                    let script = format!("window.scrollToLine({});", line);
                    let _ = webview.evaluate_script(&script);
                }
                AppEvent::ReloadFile(path) => {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let html = render_markdown(&content);
                        let parent = path.parent().unwrap_or_else(|| Path::new("/")).to_string_lossy();
                        if let (Ok(js_html), Ok(js_dir)) = (serde_json::to_string(&html), serde_json::to_string(&parent)) {
                            let script = format!("window.updateContent({}, {});", js_html, js_dir);
                            let _ = webview.evaluate_script(&script);
                        }
                    }
                }
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
