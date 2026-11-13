//! urlbox —— 把任意 URL 变成跨平台桌面窗口的最小外壳。
//!
//! 设计目标（也是选型理由）：
//!   · 不打包 Web 运行时：Windows 用 WebView2、macOS 用 WKWebView、Linux 用 WebKitGTK，
//!     全部是操作系统自带/运行时可复用的组件，二进制只有几 MB。
//!   · 一份 Rust 源码覆盖三平台，无 C/C++ 三套平台代码的维护成本。
//!   · 单窗口、单进程、无 Node/无前端构建链，属于真正的"极小桌面"。

mod config;
#[cfg(target_os = "macos")]
mod geo;

use std::error::Error;
use std::process::Command;

use config::{Config, Permissions};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use url::Url;
use wry::{NewWindowResponse, PermissionKind, PermissionResponse, WebViewBuilder};

/// 事件循环内使用的自定义事件。
enum UserEvent {
    /// 网页标题变化，交给主线程更新窗口标题。
    Title(String),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match Config::parse(args) {
        Ok(Some(cfg)) => {
            if let Err(err) = run(cfg) {
                eprintln!("urlbox: {err}");
                std::process::exit(1);
            }
        }
        // 打印了帮助或版本，正常退出
        Ok(None) => {}
        Err(err) => {
            eprintln!("urlbox: {err}\n\n运行 `urlbox --help` 查看用法。");
            std::process::exit(2);
        }
    }
}

fn run(cfg: Config) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let mut window_builder = WindowBuilder::new()
        .with_title(cfg.title.clone())
        .with_inner_size(LogicalSize::new(cfg.width, cfg.height))
        .with_min_inner_size(LogicalSize::new(cfg.min_width, cfg.min_height))
        .with_resizable(cfg.resizable)
        .with_always_on_top(cfg.always_on_top);

    if cfg.maximized {
        window_builder = window_builder.with_maximized(true);
    }
    if cfg.fullscreen {
        window_builder = window_builder.with_fullscreen(Some(tao::window::Fullscreen::Borderless(None)));
    }

    let window = window_builder.build(&event_loop)?;

    // 允许在窗口内导航的域名基线：启动 URL 的 host + 用户显式追加的 host。
    let base_host = url::Url::parse(&cfg.url)?
        .host_str()
        .map(|h| h.to_ascii_lowercase())
        .ok_or_else(|| format!("URL `{}` 缺少主机名", cfg.url))?;
    let extra_hosts: Vec<String> = cfg
        .extra_hosts
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();

    let permissions = cfg.permissions;
    let mut builder = WebViewBuilder::new()
        .with_url(cfg.url.clone())
        .with_devtools(cfg.devtools)
        .with_incognito(cfg.incognito)
        // 同源放行，跨域交给系统浏览器，避免被"劫持"进一个无从返回的窗口
        .with_navigation_handler(move |target: String| {
            should_navigate_in_app(&target, &base_host, &extra_hosts)
        })
        .with_new_window_req_handler(|target: String, _features| {
            open_in_system_browser(&target);
            NewWindowResponse::Deny
        })
        .with_permission_handler(move |kind: PermissionKind| permission_response(permissions, kind));

    if let Some(ua) = &cfg.user_agent {
        builder = builder.with_user_agent(ua.clone());
    }

    if cfg.follow_title {
        let proxy = proxy.clone();
        builder = builder.with_document_title_changed_handler(move |title: String| {
            let _ = proxy.send_event(UserEvent::Title(title));
        });
    }

    // wry 只支持一个 IPC handler，定位桥和诊断共用。
    let geo_bridge = cfg.geo_bridge;
    let diag = cfg.diag;
    #[cfg(not(target_os = "macos"))]
    let _ = geo_bridge;

    builder = builder.with_ipc_handler(move |req: wry::http::Request<String>| {
        let body = req.body().to_string();

        // 网页请求定位 → 走原生 CoreLocation 桥
        if body.contains("\"type\":\"geo\"") {
            #[cfg(target_os = "macos")]
            if geo_bridge {
                if let Some(id) = parse_geo_id(&body) {
                    geo::request(id);
                    return;
                }
            }
        }

        // 诊断输出（--diag）：写 stderr 与 /tmp/urlbox-diag.log，
        // 便于 `open dist/UrlBox.app --args --diag` 这种正常启动方式做无人值守验证。
        if diag {
            eprintln!("[urlbox-diag] {body}");
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/urlbox-diag.log")
            {
                let _ = writeln!(f, "{body}");
            }
        }
    });

    // macOS：接管 navigator.geolocation（WKWebView 原生定位在本平台不可用）
    #[cfg(target_os = "macos")]
    if cfg.geo_bridge {
        builder = builder.with_initialization_script(geo::GEO_SHIM_JS);
    }

    // 诊断脚本：主动申请定位/摄像头/麦克风并回传结果
    if cfg.diag {
        builder = builder.with_initialization_script(DIAG_SCRIPT);
    }

    // 可选诊断：URLBOX_TRACE=1 时把页面加载事件打到 stderr，便于无人值守验证。
    if std::env::var_os("URLBOX_TRACE").is_some() {
        builder = builder.with_on_page_load_handler(|event, url| {
            eprintln!("[urlbox] load {} {url}", match event {
                wry::PageLoadEvent::Started => "started",
                wry::PageLoadEvent::Finished => "finished",
            });
        });
    }

    // 平台差异：Linux 必须把 webview 塞进 GTK 的 vbox，Windows/macOS 直接挂在 window 上。
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window)?;

    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .ok_or("无法获取 GTK vbox，Linux 上需要 tao 的 x11 后端")?;
        builder.build_gtk(vbox)?
    };

    // macOS：WebView 交给定位桥持有（thread_local），同时保证其生命周期
    #[cfg(target_os = "macos")]
    geo::install(webview);

    #[cfg(not(target_os = "macos"))]
    let _webview = webview;

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Title(title)) => {
                if !title.trim().is_empty() {
                    window.set_title(&title);
                }
            }
            _ => {}
        }
    });
}

/// 判断某个导航目标是否留在窗口内。
fn should_navigate_in_app(target: &str, base_host: &str, extra_hosts: &[String]) -> bool {
    // 页面内部产生的伪协议 / 内联内容，放行交给浏览器内核处理。
    if target.starts_with("about:")
        || target.starts_with("data:")
        || target.starts_with("blob:")
        || target.starts_with("javascript:")
    {
        return true;
    }

    let parsed = match Url::parse(target) {
        Ok(url) => url,
        // 相对地址解析失败时按同源处理，避免把正常跳转误判成外链
        Err(_) => return true,
    };

    match parsed.scheme() {
        "http" | "https" => {
            let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
            if host == base_host || extra_hosts.iter().any(|h| *h == host) {
                true
            } else {
                open_in_system_browser(target);
                false
            }
        }
        "file" => true,
        // mailto: / tel: / 自定义协议：交给系统
        _ => {
            open_in_system_browser(target);
            false
        }
    }
}

/// 权限策略：网页申请权限时的应答。
///
/// 目标是"页面不再弹任何权限询问框"：
/// · `Auto`（默认）—— 网页要什么就给什么，直接 Allow；
/// · `All` —— 与 Auto 等价，保留语义便于显式声明；
/// · `None` —— 全部拒绝（做只读壳时用）。
///
/// 注意：这只能消除 **WebView 层**的询问框；系统级 TCC（macOS 摄像头/麦克风/定位）
/// 由操作系统强制，首次必须授权一次，之后对同一签名的 App 永久静默。
/// 从 IPC 消息里取出定位请求的 id：`{"type":"geo","op":"get","id":7}`
fn parse_geo_id(body: &str) -> Option<u64> {
    let idx = body.find("\"id\":")?;
    body[idx + 5..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn permission_response(policy: Permissions, _kind: PermissionKind) -> PermissionResponse {
    match policy {
        Permissions::None => PermissionResponse::Deny,
        Permissions::Auto | Permissions::All => PermissionResponse::Allow,
    }
}

/// 诊断脚本：仅在 URLBOX_DIAG=1 时注入，不参与正常运行。
const DIAG_SCRIPT: &str = r#"
(function () {
  var send = function (o) {
    try { window.ipc.postMessage(JSON.stringify(o)); } catch (e) { console.log(e); }
  };
  var checkGeo = function () {
    if (!navigator.geolocation) { send({type:'geolocation', ok:false, msg:'navigator.geolocation 不存在'}); return; }
    navigator.geolocation.getCurrentPosition(
      function (p) {
        send({type:'geolocation', ok:true, lat:+p.coords.latitude.toFixed(5),
              lng:+p.coords.longitude.toFixed(5), acc:Math.round(p.coords.accuracy)});
      },
      function (e) { send({type:'geolocation', ok:false, code:e.code, msg:e.message}); },
      {enableHighAccuracy:true, timeout:12000, maximumAge:60000}
    );
  };
  var checkMedia = function () {
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
      send({type:'media', ok:false, msg:'navigator.mediaDevices 不存在'}); return;
    }
    navigator.mediaDevices.getUserMedia({video:true, audio:true}).then(
      function (s) {
        send({type:'media', ok:true,
              tracks:s.getTracks().map(function (t) { return t.kind + ':' + (t.label || '?'); })});
        s.getTracks().forEach(function (t) { t.stop(); });
      },
      function (e) { send({type:'media', ok:false, name:e.name, msg:e.message}); }
    );
  };
  window.addEventListener('load', function () {
    setTimeout(function () { checkGeo(); checkMedia(); }, 600);
  });
})();
"#;

/// 用系统默认浏览器打开链接（跨平台实现，失败时只记录，不打断主流程）。
fn open_in_system_browser(target: &str) {
    if !(target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("tel:"))
    {
        return;
    }

    let result = if cfg!(target_os = "macos") {
        Command::new("open").arg(target).spawn()
    } else if cfg!(target_os = "windows") {
        // start 的第一个参数会被当成窗口标题，所以先给一个空标题
        Command::new("cmd").args(["/C", "start", "", target]).spawn()
    } else {
        Command::new("xdg-open").arg(target).spawn()
    };

    if let Err(err) = result {
        eprintln!("urlbox: 无法调起系统浏览器（{err}）：{target}");
    }
}
