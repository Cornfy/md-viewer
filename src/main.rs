use comrak::{markdown_to_html, ComrakOptions};
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use std::{
    env, fs,
    io::{self, BufRead},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

// Linux 平台专用的 GTK 扩展 Trait
#[cfg(target_os = "linux")]
use gtk::prelude::*;
#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcMessage {
    #[serde(rename = "content")]
    Content { markdown: String },
    #[serde(rename = "scroll")]
    Scroll { line: usize },
}

#[derive(Debug)]
enum AppEvent {
    UpdateHtml(String),
    ScrollTo(usize),
    ReloadFile(PathBuf),
}

const CSS_STYLE: &str = include_str!("../assets/github-markdown-dark.min.css");

fn render_markdown(content: &str) -> String {
    let mut options = ComrakOptions::default();
    options.render.sourcepos = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    markdown_to_html(content, &options)
}

fn generate_html_shell() -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    {CSS_STYLE}
    /* 采用响应式全宽布局，适当留出内边距 */
    body {{
      box-sizing: border-box;
      min-width: 200px;
      max-width: 100%; /* 或者 1200px，让内容随平铺窗口自适应 */
      margin: 0 auto;
      padding: 30px 45px;
      background-color: #0d1117;
    }}
    /* 平滑滚动 */
    html {{
      scroll-behavior: smooth;
    }}
    /* 代码块与表格优化：支持自身横向滚动，避免排版撕裂 */
    pre, table {{
      overflow-x: auto;
    }}
  </style>
</head>
<body class="markdown-body">
  <div id="content"></div>
  <script>
    window.updateContent = (html) => {{
      document.getElementById("content").innerHTML = html;
    }};
    
    window.scrollToLine = (line) => {{
      const target = document.querySelector(`[data-sourcepos^="${{line}}:"]`);
      if (target) {{
        target.scrollIntoView({{ behavior: "smooth", block: "center" }});
      }}
    }};
  </script>
</body>
</html>"#
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: \n  md-viewer <file.md>  (监控文件)\n  md-viewer -          (监听 STDIN)");
        std::process::exit(1);
    }

    let input_mode = args[1].clone();
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 1. 后台输入处理
    if input_mode == "-" {
        let proxy_clone = proxy.clone();
        thread::spawn(move || {
            let stdin = io::stdin();
            let handle = stdin.lock();
            for line in handle.lines().flatten() {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&line) {
                    match msg {
                        IpcMessage::Content { markdown } => {
                            let html = render_markdown(&markdown);
                            let _ = proxy_clone.send_event(AppEvent::UpdateHtml(html));
                        }
                        IpcMessage::Scroll { line } => {
                            let _ = proxy_clone.send_event(AppEvent::ScrollTo(line));
                        }
                    }
                }
            }
            std::process::exit(0);
        });
    } else {
        let file_path = PathBuf::from(&input_mode).canonicalize().expect("无效的文件路径");
        let _ = proxy.send_event(AppEvent::ReloadFile(file_path.clone()));

        let proxy_clone = proxy.clone();
        let target_filename = file_path.file_name().unwrap().to_owned();
        let parent_dir = file_path.parent().unwrap().to_owned();

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
                    thread::sleep(Duration::from_millis(50));
                    while rx.try_recv().is_ok() {}
                    let _ = proxy_clone.send_event(AppEvent::ReloadFile(file_path.clone()));
                }
            }
        });
    }

    // 2. 创建窗口
    let window = WindowBuilder::new()
        .with_title("Native Markdown Viewer")
        .with_decorations(false)
        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
        .build(&event_loop)
        .unwrap();

    let builder = WebViewBuilder::new().with_html(generate_html_shell());

    // 3. 针对 Linux (GTK) / macOS / Windows 的跨平台 Webview 构建
    #[cfg(target_os = "linux")]
    let webview = {
        let gtk_window = window.gtk_window();
        let vbox = gtk_window
            .child()
            .expect("Window 没有子容器")
            .downcast::<gtk::Box>()
            .expect("Window 子容器不是 GtkBox");
        builder.build_gtk(&vbox).unwrap()
    };

    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window).unwrap();

    // 4. GUI 事件循环
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(app_event) => match app_event {
                AppEvent::UpdateHtml(html) => {
                    if let Ok(js_string) = serde_json::to_string(&html) {
                        let script = format!("window.updateContent({});", js_string);
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
                        if let Ok(js_string) = serde_json::to_string(&html) {
                            let script = format!("window.updateContent({});", js_string);
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
