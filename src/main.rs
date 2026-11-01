#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

//! Aurora - trình duyệt Rust (tao + wry).
//!
//! Kiến trúc (lấy ý tưởng từ Zen / Min / servoshell):
//!   - `chrome`  : webview UI (thanh bên dọc + thanh công cụ), nằm dưới cùng.
//!   - mỗi tab   : một webview con, tab chưa dùng được nạp lười (lazy) khi khôi phục phiên.
//!   - `overlay` : webview nổi trên cùng cho gợi ý địa chỉ / lệnh / tìm trong trang.
//!   - Rust giữ toàn bộ trạng thái (tab, bookmark, lịch sử), UI chỉ là bộ vẽ.
//!
//! Aurora 0.3:
//!   - Một `WebContext` dùng chung cho mọi webview: cookie/bộ nhớ đệm dùng
//!     chung giữa các tab VÀ lưu bền vững => đăng nhập Google / Microsoft
//!     còn nguyên sau khi tắt mở lại trình duyệt.
//!   - Giao thức tùy chỉnh `aurora://`: trang chủ mới (đồng hồ, tìm kiếm,
//!     phím tắt), ảnh nền .png/.jpg/.gif/.webp/.mp4/.webm…, trang cài đặt.
//!   - User-agent "hình Chrome" để đăng nhập Google / Microsoft trên WebKitGTK.
//!   - Đổi công cụ tìm kiếm ngay trên trang chủ (Google / Bing / DuckDuckGo /
//!     Brave / Yandex) — hữu ích khi Google bị giới hạn.
//!   - Tăng tốc: luôn dùng GPU, canvas 2D tăng tốc, cuộn mượt, WebRTC + media.

mod config;
mod store;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod wallhttp;

use serde_json::json;
use store::{host_of, SavedTab, Store};
use tao::{
    dpi::LogicalSize as TaoSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder,
};

// FIX BUILD v0.2 -> v0.3: `build_gtk` (dùng khi dựng webview vào gtk::Fixed)
// và `webview()` (lấy WebKitGTK webview gốc để tinh chỉnh) nằm trong trait
// mở rộng của wry trên Linux — muốn dùng phải đưa trait vào scope.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use wry::{WebViewBuilderExtUnix, WebViewExtUnix};

use config::{ENGINES, SharedConfig};
use std::borrow::Cow;
use std::sync::OnceLock;

/// Cổng của máy chủ HTTP ảnh/video nền (Linux).
static WALL_PORT: OnceLock<Option<u16>> = OnceLock::new();

const KEYS: &str = include_str!("keys.js");
const PAGE: &str = include_str!("page.js");
const NEWTAB_HTML: &str = include_str!("newtab.html");
const SETTINGS_HTML: &str = include_str!("settings.html");

const TOP_H: f64 = 52.0;
const SIDEBAR_W: f64 = 248.0;
const SIDEBAR_C: f64 = 60.0;

/// Tiền tố URL nội bộ.
/// Linux/macOS: `aurora://local/<trang>`; Windows: wry ánh xạ custom protocol
/// sang `http://aurora.local/<trang>`. Trang và ảnh nền dùng chung "host"
/// để WebKit coi là cùng nguồn (giống kiến trúc của Tauri).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const INTERNAL_BASE: &str = "aurora://local/";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const INTERNAL_BASE: &str = "http://aurora.local/";

fn internal_url(page: &str) -> String {
    format!("{INTERNAL_BASE}{page}")
}

/// (id, tên lệnh, phím tắt)
const COMMANDS: &[(&str, &str, &str)] = &[
    ("newtab", "Tab mới", "Ctrl+T"),
    ("closetab", "Đóng tab", "Ctrl+W"),
    ("reopen", "Mở lại tab đã đóng", "Ctrl+Shift+T"),
    ("duplicate", "Nhân đôi tab", ""),
    ("pin", "Ghim / bỏ ghim tab", ""),
    ("bookmark", "Đánh dấu / bỏ đánh dấu trang", "Ctrl+D"),
    ("find", "Tìm trong trang", "Ctrl+F"),
    ("sidebar", "Thu gọn / mở rộng thanh bên", "Ctrl+B"),
    ("zoomin", "Phóng to", "Ctrl +"),
    ("zoomout", "Thu nhỏ", "Ctrl -"),
    ("zoomreset", "Đặt lại zoom", "Ctrl 0"),
    ("devtools", "Công cụ nhà phát triển", "F12"),
    ("print", "In trang", ""),
    ("settings", "Cài đặt (ảnh nền, tìm kiếm…)", ""),
    ("wallpaper", "Đổi ảnh nền", ""),
    ("clearhistory", "Xoá lịch sử duyệt web", ""),
];

/// Bang kiểu Min / DuckDuckGo: `!yt rust` => tìm thẳng trên YouTube.
const BANGS: &[(&str, &str)] = &[
    ("g", "https://www.google.com/search?q="),
    ("ddg", "https://duckduckgo.com/?q="),
    ("b", "https://www.bing.com/search?q="),
    ("yt", "https://www.youtube.com/results?search_query="),
    ("gh", "https://github.com/search?q="),
    ("w", "https://vi.wikipedia.org/w/index.php?search="),
    ("so", "https://stackoverflow.com/search?q="),
    ("crates", "https://crates.io/search?q="),
    ("docs", "https://docs.rs/releases/search?query="),
];

fn page_script() -> String {
    format!("{}\n{}", KEYS, PAGE)
}
fn chrome_html() -> String {
    include_str!("ui.html").replace("/*KEYS*/", KEYS)
}

// ---------------------------------------------------------------------------
// Sự kiện
// ---------------------------------------------------------------------------

enum TabMsg {
    Title(String),
    Url(String),
    Loading(bool),
    Ipc(String),
}

enum UserEvent {
    Chrome(String),
    Overlay(String),
    Tab(u32, TabMsg),
    /// Người dùng vừa chọn tệp ảnh nền từ hộp thoại hệ thống.
    Wallpaper(Option<String>),
}

// ---------------------------------------------------------------------------
// Tiện ích URL
// ---------------------------------------------------------------------------

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Trả về (url, là_tìm_kiếm). `search` là URL của công cụ tìm kiếm đang chọn.
fn resolve(input: &str, search: &str) -> (String, bool) {
    let t = input.trim();
    if t.is_empty() {
        return (String::new(), false);
    }
    if t.contains("://") || t.starts_with("about:") || t.starts_with("file:") {
        return (t.to_string(), false);
    }
    let host = t.split('/').next().unwrap_or("");
    let looks_like_host =
        !t.contains(' ') && (host.contains('.') || host.starts_with("localhost") || host.contains(':'));
    if looks_like_host {
        let local = host.starts_with("localhost")
            || host.starts_with("127.")
            || host.starts_with("192.168.")
            || host.starts_with("10.");
        (format!("{}://{}", if local { "http" } else { "https" }, t), false)
    } else {
        (format!("{}{}", search, urlencode(t)), true)
    }
}

/// URL nội bộ (trang chủ, cài đặt) thì coi như rỗng.
fn display_url(u: &str) -> String {
    if u.starts_with("about:")
        || u.starts_with("data:")
        || u.starts_with("wry:")
        || u.contains("wry.localhost")
        || u.starts_with("aurora://")
        || u.starts_with("http://aurora.")
    {
        String::new()
    } else {
        u.to_string()
    }
}

// ---------------------------------------------------------------------------
// Giao thức nội bộ `aurora://` : trang chủ, ảnh nền, cài đặt
// ---------------------------------------------------------------------------

fn internal_response(status: u16, mime: &str, body: Vec<u8>) -> wry::http::Response<Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-cache")
        .body(Cow::Owned(body))
        .unwrap_or_else(|_| wry::http::Response::new(Cow::Borrowed(&b""[..])))
}

/// Lấy "tuyến" từ URI:
/// `aurora://local/newtab` / `http://aurora.local/newtab` => `newtab`
/// (dự phòng khi URL không có path: `aurora://newtab` / `http://aurora.newtab`).
fn route_of(uri: &wry::http::Uri) -> String {
    let path = uri.path().trim_matches('/');
    if !path.is_empty() {
        return path.to_string();
    }
    let raw = uri.host().map(str::to_string).unwrap_or_default();
    let raw = raw.trim_matches('.');
    raw.strip_prefix("aurora.").unwrap_or(raw).to_string()
}

/// Phục vụ nội bộ: trang chủ / ảnh nền / trang cài đặt.
fn serve_internal(c: &config::Config, uri: &wry::http::Uri, port: Option<u16>) -> wry::http::Response<Cow<'static, [u8]>> {
    let route = route_of(uri);
    if std::env::var("AURORA_DEBUG").is_ok() {
        eprintln!("[aurora] aurora-scheme: {uri} -> {route}");
    }
    match route.as_str() {
"newtab" => internal_response(200, "text/html; charset=utf-8", render_newtab(c, port).into_bytes()),
        "settings" => internal_response(200, "text/html; charset=utf-8", render_settings(c, port).into_bytes()),
        "wallpaper" => {
            let path = c.wallpaper.as_deref().unwrap_or("");
            match (config::wallpaper_mime(path), std::fs::read(path)) {
                (Some(mime), Ok(bytes)) => internal_response(200, mime, bytes),
                _ => internal_response(404, "text/plain; charset=utf-8", b"khong co nen".to_vec()),
            }
        }
        _ => internal_response(404, "text/plain; charset=utf-8", b"khong tim thay".to_vec()),
    }
}

/// URL của tệp nền. Video trên Linux đi qua HTTP cục bộ vì WebKitGTK không
/// phát media từ custom scheme; các loại khác dùng `aurora://`.
fn wallpaper_url(mime: &str, v: u64, port: Option<u16>) -> String {
    let direct = format!("{INTERNAL_BASE}wallpaper?v={v}");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if mime.starts_with("video/") {
        if let Some(p) = port {
            return format!("http://127.0.0.1:{p}/wallpaper?v={v}");
        }
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let _ = port;
    direct
}

/// (url ảnh nền kèm số phiên bản, mime) — rỗng nếu chưa đặt hoặc tệp không đọc được.
fn wallpaper_info(c: &config::Config, port: Option<u16>) -> (String, &'static str) {
    let Some(path) = c.wallpaper.as_deref() else {
        return (String::new(), "");
    };
    let Some(mime) = config::wallpaper_mime(path) else {
        return (String::new(), "");
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return (String::new(), "");
    };
    let v = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (wallpaper_url(mime, v, port), mime)
}

fn engines_json() -> serde_json::Value {
    ENGINES
        .iter()
        .map(|e| json!({ "id": e.id, "name": e.name }))
        .collect::<Vec<_>>()
        .into()
}

fn render_newtab(c: &config::Config, port: Option<u16>) -> String {
    let engine = config::engine_of(&c.engine);
    let (wp, mime) = wallpaper_info(c, port);
    let cfg = json!({
        "base": INTERNAL_BASE,
        "wallpaper": wp,
        "wpmime": mime,
        "engine": engine.id,
        "engines": engines_json(),
        "settings": internal_url("settings"),
        "nativePicker": cfg!(not(any(target_os = "windows", target_os = "macos"))),
    });
    NEWTAB_HTML.replace("/*CFG*/null", &cfg.to_string())
}

fn render_settings(c: &config::Config, port: Option<u16>) -> String {
    let engine = config::engine_of(&c.engine);
    let (wp, mime) = wallpaper_info(c, port);
    let cfg = json!({
        "base": INTERNAL_BASE,
        "wallpaper": wp,
        "wpmime": mime,
        "engine": engine.id,
        "engines": engines_json(),
        "ua": c.ua_compat,
        "gpu": c.gpu_always,
        "home": internal_url("newtab"),
        "nativePicker": cfg!(not(any(target_os = "windows", target_os = "macos"))),
    });
    SETTINGS_HTML.replace("/*CFG*/null", &cfg.to_string())
}

// ---------------------------------------------------------------------------
// Host: gắn webview con vào cửa sổ (khác nhau giữa Windows/macOS và Linux)
// ---------------------------------------------------------------------------

struct Host {
    window: Window,
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fixed: gtk::Fixed,
}

impl Host {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn attach(&self, b: WebViewBuilder, bounds: Rect) -> WebView {
        b.with_bounds(bounds)
            .build_as_child(&self.window)
            .expect("không tạo được webview")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn attach(&self, b: WebViewBuilder, bounds: Rect) -> WebView {
        b.with_bounds(bounds)
            .build_gtk(&self.fixed)
            .expect("không tạo được webview")
    }
}

/// Tinh chỉnh WebKitGTK cho mượt và nhanh hơn (chỉ Linux):
/// canvas 2D tăng tốc GPU, cuộn mượt, WebRTC/media cho gọi video,
/// và chính sách "luôn dùng GPU" nếu bật trong cấu hình.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn tune_webview(wv: &WebView, c: &config::Config) {
    use webkit2gtk::{SettingsExt, WebViewExt};
    let wk = wv.webview();
    if let Some(s) = wk.settings() {
        // (canvas 2D tăng tốc là mặc định từ WebKit 2.32)
        s.set_enable_smooth_scrolling(true);
        s.set_enable_media_stream(true);
        s.set_enable_webrtc(true);
        if c.gpu_always {
            s.set_hardware_acceleration_policy(webkit2gtk::HardwareAccelerationPolicy::Always);
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn tune_webview(_wv: &WebView, _c: &config::Config) {}

/// Tệp tải về lưu vào ~/Downloads, tự đổi tên khi trùng.
fn unique_download_path(mut p: std::path::PathBuf) -> std::path::PathBuf {
    if !p.exists() {
        return p;
    }
    let parent = p.parent().map(|x| x.to_path_buf()).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut i = 1;
    while p.exists() {
        p = parent.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    p
}

/// Overlay luôn phải nằm trên cùng, nên được dựng lại mỗi khi có tab mới.
fn make_overlay(host: &Host, ctx: &mut WebContext, proxy: &EventLoopProxy<UserEvent>) -> WebView {
    let p = proxy.clone();
    host.attach(
        WebViewBuilder::with_web_context(ctx)
            .with_transparent(true)
            .with_visible(false)
            .with_html(include_str!("overlay.html"))
            .with_ipc_handler(move |req| {
                let _ = p.send_event(UserEvent::Overlay(req.body().clone()));
            }),
        rect(0.0, 0.0, 10.0, 10.0),
    )
}

// ---------------------------------------------------------------------------
// Trạng thái
// ---------------------------------------------------------------------------

struct Tab {
    id: u32,
    webview: WebView,
    title: String,
    url: String,
    icon: String,
    loading: bool,
    pinned: bool,
    zoom: f64,
    /// Tab khôi phục từ phiên cũ: chỉ nạp khi người dùng chuyển tới.
    pending: Option<String>,
}

#[derive(Clone)]
struct Item {
    kind: &'static str,
    title: String,
    sub: String,
    url: String,
    action: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Hidden,
    Suggest,
    Find,
}

struct App {
    host: Host,
    chrome: WebView,
    overlay: WebView,
    tabs: Vec<Tab>,
    active: u32,
    next_id: u32,
    proxy: EventLoopProxy<UserEvent>,
    quit: bool,
    store: Store,
    collapsed: bool,
    items: Vec<Item>,
    sel: Option<usize>,
    mode: Mode,
    closed: Vec<String>,
    visit_count: u32,
    /// Cấu hình dùng chung (tìm kiếm, ảnh nền, UA, GPU).
    config: SharedConfig,
    /// WebContext dùng chung: cookie + cache + storage lưu bền vững.
    web_context: WebContext,
    /// Đang nhận tệp ảnh nền đẩy từ trang cài đặt (Windows/macOS).
    wp_out: Option<(std::fs::File, String)>,
}

impl App {
    // ----- hình học -------------------------------------------------------

    fn win_size(&self) -> (f64, f64) {
        let sf = self.host.window.scale_factor();
        let s = self.host.window.inner_size().to_logical::<f64>(sf);
        (s.width, s.height)
    }

    fn sidebar_w(&self) -> f64 {
        if self.collapsed {
            SIDEBAR_C
        } else {
            SIDEBAR_W
        }
    }

    fn pill_w(content_w: f64) -> f64 {
        (content_w - 220.0).clamp(240.0, 680.0)
    }

    fn layout(&self) {
        let (w, h) = self.win_size();
        let sw = self.sidebar_w();
        let cw = (w - sw).max(1.0);
        let ch = (h - TOP_H).max(1.0);
        let _ = self.chrome.set_bounds(rect(0.0, 0.0, w, h));
        for t in &self.tabs {
            let _ = t.webview.set_bounds(rect(sw, TOP_H, cw, ch));
        }
        self.place_overlay();
        let _ = self.chrome.evaluate_script(&format!(
            "window.__layout&&window.__layout({},{},{})",
            sw,
            Self::pill_w(cw),
            TOP_H
        ));
    }

    fn place_overlay(&self) {
        let (w, _h) = self.win_size();
        let sw = self.sidebar_w();
        let cw = (w - sw).max(1.0);
        match self.mode {
            Mode::Hidden => {
                let _ = self.overlay.set_visible(false);
            }
            Mode::Suggest => {
                let pw = Self::pill_w(cw);
                let rows = self.items.len().min(9) as f64;
                let _ = self
                    .overlay
                    .set_bounds(rect(sw + (cw - pw) / 2.0, TOP_H - 4.0, pw, rows * 40.0 + 18.0));
                let _ = self.overlay.set_visible(true);
            }
            Mode::Find => {
                let ow = 400.0_f64.min(cw - 16.0);
                let _ = self
                    .overlay
                    .set_bounds(rect(sw + cw - ow - 12.0, TOP_H + 8.0, ow, 50.0));
                let _ = self.overlay.set_visible(true);
            }
        }
    }

    // ----- truy cập nhanh -------------------------------------------------

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == self.active)
    }

    fn eval_active(&self, js: &str) {
        if let Some(t) = self.active_tab() {
            let _ = t.webview.evaluate_script(js);
        }
    }

    fn focus_tab(&self) {
        if let Some(t) = self.active_tab() {
            let _ = t.webview.focus();
        }
    }

    fn focus_url(&self) {
        let _ = self.chrome.focus();
        let _ = self.chrome.evaluate_script("window.focusUrl&&window.focusUrl()");
    }

    fn search_url(&self) -> String {
        let c = self.config.lock().unwrap();
        config::engine_of(&c.engine).url.to_string()
    }

    fn search_name(&self) -> String {
        let c = self.config.lock().unwrap();
        config::engine_of(&c.engine).name.to_string()
    }

    // ----- đẩy trạng thái xuống UI ---------------------------------------

    fn push_state(&self) {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .map(|t| {
                json!({
                    "id": t.id, "title": t.title, "url": t.url, "icon": t.icon,
                    "loading": t.loading, "pinned": t.pinned
                })
            })
            .collect();
        let bms: Vec<_> = self
            .store
            .bookmarks
            .iter()
            .take(30)
            .map(|b| json!({ "title": b.title, "url": b.url }))
            .collect();
        let cur = self.active_tab().map(|t| t.url.clone()).unwrap_or_default();
        let state = json!({
            "tabs": tabs, "active": self.active, "bms": bms,
            "bm": self.store.is_bookmarked(&cur), "collapsed": self.collapsed
        });
        let _ = self.chrome.evaluate_script(&format!("window.__state({state})"));
    }

    fn push_home(&self, tab: &Tab) {
        let top: Vec<_> = self
            .store
            .top_sites(8)
            .into_iter()
            .map(|v| json!({ "title": if v.title.is_empty() { host_of(&v.url) } else { v.title }, "url": v.url }))
            .collect();
        let bms: Vec<_> = self
            .store
            .bookmarks
            .iter()
            .take(8)
            .map(|b| json!({ "title": b.title, "url": b.url }))
            .collect();
        let d = json!({ "top": top, "bms": bms });
        let _ = tab.webview.evaluate_script(&format!("window.__home&&window.__home({d})"));
    }

    // ----- tab ------------------------------------------------------------

    fn open_tab(&mut self, url: Option<String>, title: Option<String>, pinned: bool, activate: bool) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let load_now = activate && url.is_some();

        let (w, h) = self.win_size();
        let sw = self.sidebar_w();

        let (p_title, p_load, p_ipc) = (self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let script = page_script();
        let cfg = self.config.lock().unwrap().clone();
        let scheme_cfg = self.config.clone();
        let scheme_port = WALL_PORT.get().copied().flatten();

        let mut b = WebViewBuilder::with_web_context(&mut self.web_context)
            // Giao thức nội bộ: aurora://newtab | aurora://wallpaper | aurora://settings
            .with_custom_protocol("aurora".into(), move |_id, request| {
                let c = scheme_cfg.lock().unwrap().clone();
                serve_internal(&c, request.uri(), scheme_port)
            })
            .with_devtools(true)
            .with_back_forward_navigation_gestures(true)
            // cho phép nền .mp4/.webm tự phát, copy/paste trong trang
            .with_autoplay(true)
            .with_clipboard(true)
            .with_initialization_script(&script)
            .with_document_title_changed_handler(move |t| {
                let _ = p_title.send_event(UserEvent::Tab(id, TabMsg::Title(t)));
            })
            .with_on_page_load_handler(move |ev, u| {
                let _ = p_load.send_event(UserEvent::Tab(id, TabMsg::Url(u)));
                let loading = matches!(ev, PageLoadEvent::Started);
                let _ = p_load.send_event(UserEvent::Tab(id, TabMsg::Loading(loading)));
            })
            .with_ipc_handler(move |req| {
                let _ = p_ipc.send_event(UserEvent::Tab(id, TabMsg::Ipc(req.body().clone())));
            });

        // UA "hình Chrome" => Google/Microsoft cho đăng nhập trên WebKitGTK.
        if cfg.ua_compat {
            b = b.with_user_agent(config::compat_ua());
        }

        b = match (&url, load_now) {
            (Some(u), true) => b.with_url(u),
            _ => b.with_url(internal_url("newtab")),
        };

        let webview = self.host.attach(b, rect(sw, TOP_H, w - sw, h - TOP_H));
        tune_webview(&webview, &cfg);
        if !activate {
            let _ = webview.set_visible(false);
        }

        let pending = if url.is_some() && !load_now { url.clone() } else { None };
        self.tabs.push(Tab {
            id,
            webview,
            title: title.filter(|t| !t.is_empty()).unwrap_or_else(|| "Tab mới".into()),
            url: url.clone().map(|u| display_url(&u)).unwrap_or_default(),
            icon: String::new(),
            loading: load_now,
            pinned,
            zoom: 1.0,
            pending,
        });
        self.tabs.sort_by_key(|t| !t.pinned);
        if activate {
            self.switch_to(id);
        }
        id
    }

    /// Mở tab và dựng lại overlay để nó luôn nằm trên cùng.
    fn new_tab(&mut self, url: Option<String>) {
        self.hide_overlay();
        self.open_tab(url, None, false, true);
        self.rebuild_overlay();
        self.save_session();
    }

    fn rebuild_overlay(&mut self) {
        self.overlay = make_overlay(&self.host, &mut self.web_context, &self.proxy);
        self.mode = Mode::Hidden;
    }

    fn switch_to(&mut self, id: u32) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else { return };
        self.hide_overlay();
        self.active = id;
        if let Some(u) = self.tabs[idx].pending.take() {
            self.tabs[idx].loading = true;
            let _ = self.tabs[idx].webview.load_url(&u);
        }
        for t in &self.tabs {
            let _ = t.webview.set_visible(t.id == id);
        }
        let _ = self.tabs[idx].webview.focus();
        self.host.window.set_title(&format!("{} — Aurora", self.tabs[idx].title));
        self.push_state();
    }

    fn close_tab(&mut self, id: u32) {
        let Some(pos) = self.tabs.iter().position(|t| t.id == id) else { return };
        let closed = self.tabs.remove(pos);
        if !closed.url.is_empty() {
            self.closed.push(closed.url.clone());
            if self.closed.len() > 20 {
                self.closed.remove(0);
            }
        }
        drop(closed);
        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        if self.active == id {
            let next = self.tabs[pos.min(self.tabs.len() - 1)].id;
            self.switch_to(next);
        } else {
            self.push_state();
        }
        self.save_session();
    }

    fn cycle(&mut self, delta: isize) {
        if self.tabs.len() < 2 {
            return;
        }
        let cur = self.tabs.iter().position(|t| t.id == self.active).unwrap_or(0) as isize;
        let n = self.tabs.len() as isize;
        let next = self.tabs[(cur + delta).rem_euclid(n) as usize].id;
        self.switch_to(next);
    }

    fn toggle_pin(&mut self, id: u32) {
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.pinned = !t.pinned;
        }
        self.tabs.sort_by_key(|t| !t.pinned);
        self.push_state();
        self.save_session();
    }

    fn reorder(&mut self, id: u32, idx: usize) {
        if let Some(from) = self.tabs.iter().position(|t| t.id == id) {
            let t = self.tabs.remove(from);
            let idx = idx.min(self.tabs.len());
            self.tabs.insert(idx, t);
            self.tabs.sort_by_key(|t| !t.pinned);
            self.push_state();
            self.save_session();
        }
    }

    fn navigate(&mut self, id: u32, text: &str) {
        let search = self.search_url();
        let (url, _) = resolve(text, &search);
        if url.is_empty() {
            return;
        }
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.url = display_url(&url);
            t.loading = true;
            t.pending = None;
            let _ = t.webview.load_url(&url);
        }
        self.push_state();
    }

    fn zoom_by(&mut self, delta: f64, reset: bool) {
        let id = self.active;
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            t.zoom = if reset {
                1.0
            } else {
                (((t.zoom + delta) * 10.0).round() / 10.0).clamp(0.3, 3.0)
            };
            let _ = t.webview.zoom(t.zoom);
        }
    }

    fn toggle_bookmark(&mut self) {
        let Some(t) = self.active_tab() else { return };
        if t.url.is_empty() {
            return;
        }
        let (u, title) = (t.url.clone(), t.title.clone());
        self.store.toggle_bookmark(&u, &title);
        self.push_state();
    }

    // ----- cài đặt (tìm kiếm, ảnh nền, UA, GPU) --------------------------

    fn set_engine(&mut self, id: &str) {
        if ENGINES.iter().any(|e| e.id == id) {
            if let Ok(mut c) = self.config.lock() {
                c.engine = id.to_string();
                config::save(&c);
            }
        }
    }

    fn set_flag(&mut self, ua: Option<bool>, gpu: Option<bool>) {
        if let Ok(mut c) = self.config.lock() {
            if let Some(v) = ua {
                c.ua_compat = v;
            }
            if let Some(v) = gpu {
                c.gpu_always = v;
            }
            config::save(&c);
        }
    }

    fn set_wallpaper(&mut self, path: Option<String>) {
        if let Ok(mut c) = self.config.lock() {
            c.wallpaper = path;
            config::save(&c);
        }
        self.reload_internal();
    }

    /// Nạp lại mọi tab nội bộ (trang chủ / cài đặt) để áp dụng nền mới.
    fn reload_internal(&self) {
        for t in &self.tabs {
            if t.url.is_empty() {
                let _ = t.webview.evaluate_script("location.reload()");
            }
        }
    }

    /// Hộp thoại chọn ảnh/video nền của hệ thống (Linux).
    fn pick_wallpaper(&mut self) {
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            use gtk::prelude::*;
            use gtk::{FileChooserAction, FileChooserNative, FileFilter, ResponseType};

            let anc = self.host.fixed.ancestor(gtk::Window::static_type());
            let parent = anc.as_ref().and_then(|w| w.downcast_ref::<gtk::Window>());
            let dialog = FileChooserNative::new(
                Some("Chọn ảnh hoặc video làm nền"),
                parent,
                FileChooserAction::Open,
                Some("Chọn"),
                Some("Huỷ"),
            );
            let filter = FileFilter::new();
            filter.set_name(Some("Ảnh & video (png, jpg, gif, webp, mp4, webm…)"));
            filter.add_pixbuf_formats();
            for m in ["video/mp4", "video/webm", "video/x-matroska", "video/quicktime"] {
                filter.add_mime_type(m);
            }
            dialog.add_filter(filter);

            let proxy = self.proxy.clone();
            dialog.connect_response(move |dlg, resp| {
                if resp == ResponseType::Accept {
                    if let Some(p) = dlg.filename() {
                        let _ = proxy.send_event(UserEvent::Wallpaper(Some(
                            p.to_string_lossy().into_owned(),
                        )));
                    }
                }
                dlg.destroy();
            });
            dialog.show();
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.new_tab(Some(internal_url("settings")));
        }
    }

    /// Nhận ảnh nền đẩy theo từng phần từ trang cài đặt (Windows/macOS).
    fn wp_begin(&mut self, name: &str) {
        let ext = name.rsplit('.').next().unwrap_or("img").to_lowercase();
        let path = store::dir().join("wallpaper.partial");
        self.wp_out = std::fs::File::create(&path).ok().map(|f| (f, ext));
    }

    fn wp_chunk(&mut self, b64: &str) {
        use std::io::Write;
        if let Some((f, _)) = self.wp_out.as_mut() {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                let _ = f.write_all(&bytes);
            }
        }
    }

    fn wp_end(&mut self) {
        if let Some((_f, ext)) = self.wp_out.take() {
            drop(_f);
            let ext = if config::wallpaper_mime(&format!("x.{ext}")).is_some() { ext } else { "png".to_string() };
            let src = store::dir().join("wallpaper.partial");
            let dst = store::dir().join(format!("wallpaper.{ext}"));
            if std::fs::rename(&src, &dst).is_ok() {
                self.set_wallpaper(Some(dst.to_string_lossy().into_owned()));
            }
        }
    }

    // ----- hành động chung (phím tắt + thanh lệnh) -----------------------

    fn act(&mut self, a: &str) {
        match a {
            "newtab" => {
                self.new_tab(None);
                self.focus_url();
            }
            "closetab" => self.close_tab(self.active),
            "reopen" => {
                if let Some(u) = self.closed.pop() {
                    self.new_tab(Some(u));
                }
            }
            "duplicate" => {
                if let Some(u) = self.active_tab().map(|t| t.url.clone()) {
                    if !u.is_empty() {
                        self.new_tab(Some(u));
                    }
                }
            }
            "pin" => {
                let id = self.active;
                self.toggle_pin(id);
            }
            "focusurl" => self.focus_url(),
            "reload" => self.eval_active("location.reload()"),
            "back" => self.eval_active("history.back()"),
            "fwd" => self.eval_active("history.forward()"),
            "find" => self.open_find(),
            "bookmark" => self.toggle_bookmark(),
            "sidebar" => {
                self.collapsed = !self.collapsed;
                self.layout();
                self.push_state();
                self.save_session();
            }
            "zoomin" => self.zoom_by(0.1, false),
            "zoomout" => self.zoom_by(-0.1, false),
            "zoomreset" => self.zoom_by(0.0, true),
            "devtools" => {
                if let Some(t) = self.active_tab() {
                    t.webview.open_devtools();
                }
            }
            "print" => self.eval_active("window.print()"),
            "settings" => {
                self.new_tab(Some(internal_url("settings")));
            }
            "wallpaper" => self.pick_wallpaper(),
            "nexttab" => self.cycle(1),
            "prevtab" => self.cycle(-1),
            "clearhistory" => {
                self.store.history.clear();
                self.store.save_history();
            }
            "pageclick" => {
                if self.mode == Mode::Suggest {
                    self.hide_overlay();
                }
            }
            s if s.starts_with("tab") => {
                if let Ok(n) = s[3..].parse::<usize>() {
                    if n >= 1 {
                        let idx = if n == 9 { self.tabs.len().saturating_sub(1) } else { n - 1 };
                        if let Some(t) = self.tabs.get(idx) {
                            let id = t.id;
                            self.switch_to(id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // ----- overlay: gợi ý + tìm trong trang ------------------------------

    fn hide_overlay(&mut self) {
        if self.mode == Mode::Hidden {
            return;
        }
        self.mode = Mode::Hidden;
        self.items.clear();
        self.sel = None;
        let _ = self.overlay.set_visible(false);
    }

    fn build_items(&self, text: &str) -> Vec<Item> {
        let t = text.trim();
        let mut out: Vec<Item> = Vec::new();

        // Thanh lệnh: ">" (hoặc "!" không phải bang)
        if let Some(rest) = t.strip_prefix('>') {
            let q = rest.trim().to_lowercase();
            for (id, title, key) in COMMANDS {
                if q.is_empty() || title.to_lowercase().contains(&q) {
                    out.push(Item { kind: "command", title: (*title).into(), sub: (*key).into(), url: String::new(), action: (*id).into() });
                }
            }
            return out;
        }

        if let Some(rest) = t.strip_prefix('!') {
            let mut it = rest.splitn(2, ' ');
            let name = it.next().unwrap_or("");
            let q = it.next().unwrap_or("").trim();
            if let Some((_, base)) = BANGS.iter().find(|(n, _)| *n == name) {
                if !q.is_empty() {
                    out.push(Item {
                        kind: "bang",
                        title: format!("Tìm “{q}” với !{name}"),
                        sub: host_of(base),
                        url: format!("{}{}", base, urlencode(q)),
                        action: String::new(),
                    });
                    return out;
                }
            }
            for (n, base) in BANGS {
                if n.starts_with(name) {
                    out.push(Item { kind: "hint", title: format!("!{n} từ khoá"), sub: host_of(base), url: String::new(), action: String::new() });
                }
            }
            return out;
        }

        if t.is_empty() {
            out.push(Item {
                kind: "hint",
                title: "Gõ > để xem lệnh, !g / !yt / !gh để tìm nhanh".into(),
                sub: String::new(),
                url: String::new(),
                action: String::new(),
            });
        } else {
            let (url, is_search) = resolve(t, &self.search_url());
            if is_search {
                out.push(Item { kind: "search", title: format!("Tìm “{t}”"), sub: self.search_name(), url, action: String::new() });
            } else {
                out.push(Item { kind: "go", title: url.clone(), sub: "Mở trang".into(), url, action: String::new() });
            }
        }
        for h in self.store.suggest(t, 7) {
            let title = if h.title.is_empty() { host_of(&h.url) } else { h.title };
            out.push(Item { kind: h.kind, title, sub: host_of(&h.url), url: h.url, action: String::new() });
        }
        out
    }

    fn show_suggest(&mut self) {
        if self.items.is_empty() {
            self.hide_overlay();
            return;
        }
        self.mode = Mode::Suggest;
        self.place_overlay();
        self.render_suggest();
    }

    fn render_suggest(&self) {
        let items: Vec<_> = self
            .items
            .iter()
            .map(|i| json!({ "k": i.kind, "t": i.title, "s": i.sub }))
            .collect();
        let payload = json!({ "items": items, "sel": self.sel });
        let _ = self.overlay.evaluate_script(&format!("window.__suggest({payload})"));
    }

    fn on_typing(&mut self, text: &str) {
        self.items = self.build_items(text);
        self.sel = None;
        self.show_suggest();
    }

    fn on_move(&mut self, d: i32) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        self.sel = Some(match (self.sel, d > 0) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        });
        self.render_suggest();
    }

    fn on_enter(&mut self, text: &str) {
        let t = text.trim();
        let item = match self.sel {
            Some(i) => self.items.get(i).cloned(),
            None if t.starts_with('>') || t.starts_with('!') => self.items.first().cloned(),
            None => None,
        };
        self.hide_overlay();
        match item {
            Some(it) => self.activate(it),
            None => {
                let id = self.active;
                self.navigate(id, text);
            }
        }
        self.focus_tab();
    }

    fn activate(&mut self, it: Item) {
        match it.kind {
            "command" => self.act(&it.action),
            "hint" => {}
            _ => {
                let id = self.active;
                self.navigate(id, &it.url);
            }
        }
    }

    fn open_find(&mut self) {
        self.items.clear();
        self.sel = None;
        self.mode = Mode::Find;
        self.place_overlay();
        let _ = self.overlay.focus();
        let _ = self.overlay.evaluate_script("window.__find&&window.__find()");
    }

    fn do_find(&self, text: &str, back: bool) {
        if text.is_empty() {
            return;
        }
        let q = serde_json::to_string(text).unwrap_or_default();
        self.eval_active(&format!("window.find({q},false,{back},true)"));
    }

    // ----- phiên làm việc -------------------------------------------------

    fn save_session(&mut self) {
        let saved: Vec<SavedTab> = self
            .tabs
            .iter()
            .filter(|t| !t.url.is_empty())
            .map(|t| SavedTab { url: t.url.clone(), title: t.title.clone(), pinned: t.pinned })
            .collect();
        let active = self
            .tabs
            .iter()
            .filter(|t| !t.url.is_empty())
            .position(|t| t.id == self.active)
            .unwrap_or(0);
        self.store.session.tabs = saved;
        self.store.session.active = active;
        self.store.session.sidebar_collapsed = self.collapsed;
        self.store.save_session();
    }

    fn persist(&mut self) {
        self.save_session();
        self.store.save_history();
        self.store.save_bookmarks();
    }

    fn restore(&mut self) {
        let saved = std::mem::take(&mut self.store.session.tabs);
        let active = self.store.session.active;
        if saved.is_empty() {
            self.open_tab(None, None, false, true);
        } else {
            let a = active.min(saved.len() - 1);
            for (i, s) in saved.into_iter().enumerate() {
                self.open_tab(Some(s.url), Some(s.title), s.pinned, i == a);
            }
        }
        self.rebuild_overlay();
        self.layout();
        self.push_state();
    }

    // ----- phân phối sự kiện ---------------------------------------------

    fn handle(&mut self, ev: UserEvent) {
        match ev {
            UserEvent::Chrome(m) => self.chrome_cmd(&m),
            UserEvent::Overlay(m) => self.overlay_cmd(&m),
            UserEvent::Tab(id, msg) => self.tab_event(id, msg),
            UserEvent::Wallpaper(path) => self.set_wallpaper(path),
        }
    }

    fn chrome_cmd(&mut self, msg: &str) {
        let (cmd, arg) = msg.split_once('|').unwrap_or((msg, ""));
        match cmd {
            "ready" => {
                self.layout();
                self.push_state();
                if self.active_tab().map(|t| t.url.is_empty()).unwrap_or(false) {
                    self.focus_url();
                }
            }
            "act" => self.act(arg),
            "close" => {
                if let Ok(id) = arg.parse() {
                    self.close_tab(id);
                }
            }
            "switch" => {
                if let Ok(id) = arg.parse() {
                    self.switch_to(id);
                }
            }
            "pin" => {
                if let Ok(id) = arg.parse() {
                    self.toggle_pin(id);
                }
            }
            "reorder" => {
                if let Some((a, b)) = arg.split_once('|') {
                    if let (Ok(id), Ok(idx)) = (a.parse(), b.parse()) {
                        self.reorder(id, idx);
                    }
                }
            }
            "go" => {
                let id = self.active;
                self.navigate(id, arg);
                self.focus_tab();
            }
            "focus" => self.on_typing(""),
            "typing" => self.on_typing(arg),
            "move" => self.on_move(arg.parse().unwrap_or(1)),
            "enter" => self.on_enter(arg),
            "esc" => {
                self.hide_overlay();
                self.focus_tab();
            }
            _ => {}
        }
    }

    fn overlay_cmd(&mut self, msg: &str) {
        let (cmd, arg) = msg.split_once('|').unwrap_or((msg, ""));
        match cmd {
            "pick" => {
                if let Ok(i) = arg.parse::<usize>() {
                    if let Some(it) = self.items.get(i).cloned() {
                        self.hide_overlay();
                        self.activate(it);
                        self.focus_tab();
                    }
                }
            }
            "find" => self.do_find(arg, false),
            "findnext" => {
                let mut p = arg.rsplitn(2, '|');
                let back = p.next() == Some("1");
                let text = p.next().unwrap_or("");
                self.do_find(text, back);
            }
            "findclose" => {
                self.hide_overlay();
                self.eval_active("getSelection().removeAllRanges()");
                self.focus_tab();
            }
            _ => {}
        }
    }

    fn tab_event(&mut self, id: u32, msg: TabMsg) {
        match msg {
            TabMsg::Ipc(m) => {
                let (cmd, arg) = m.split_once('|').unwrap_or((m.as_str(), ""));
                match cmd {
                    "act" => self.act(arg),
                    "open" => self.new_tab(Some(arg.to_string())),
                    "nav" => self.navigate(id, arg),
                    "icon" => {
                        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                            if t.pending.is_none() {
                                t.icon = arg.to_string();
                            }
                        }
                        self.push_state();
                    }
                    // ----- cài đặt từ trang chủ / trang cài đặt -----
                    "setengine" => self.set_engine(arg),
                    "setua" => self.set_flag(Some(arg == "1"), None),
                    "setgpu" => self.set_flag(None, Some(arg == "1")),
                    "pickwallpaper" => self.pick_wallpaper(),
                    "clearwallpaper" => self.set_wallpaper(None),
                    "wpbegin" => self.wp_begin(arg),
                    "wpchunk" => self.wp_chunk(arg),
                    "wpend" => self.wp_end(),
                    _ => {}
                }
            }
            TabMsg::Title(t) => {
                let mut hist: Option<(String, String)> = None;
                let mut is_active = false;
                if let Some(tab) = self.tabs.iter_mut().find(|x| x.id == id) {
                    if tab.pending.is_some() {
                        return;
                    }
                    tab.title = if t.trim().is_empty() { "Đang tải…".into() } else { t };
                    hist = Some((tab.url.clone(), tab.title.clone()));
                    is_active = tab.id == self.active;
                }
                if let Some((u, title)) = hist {
                    if is_active {
                        self.host.window.set_title(&format!("{title} — Aurora"));
                    }
                    if !u.is_empty() {
                        self.store.set_title(&u, &title);
                    }
                }
                self.push_state();
            }
            TabMsg::Url(u) => {
                if let Some(tab) = self.tabs.iter_mut().find(|x| x.id == id) {
                    if tab.pending.is_some() {
                        return;
                    }
                    let new = display_url(&u);
                    if new != tab.url {
                        tab.icon.clear();
                    }
                    tab.url = new;
                }
                self.push_state();
            }
            TabMsg::Loading(l) => {
                let mut record: Option<(String, String)> = None;
                let mut home = false;
                if let Some(tab) = self.tabs.iter_mut().find(|x| x.id == id) {
                    if tab.pending.is_some() {
                        return;
                    }
                    tab.loading = l;
                    if !l {
                        if tab.url.starts_with("http") {
                            record = Some((tab.url.clone(), tab.title.clone()));
                        } else if tab.url.is_empty() {
                            home = true;
                        }
                    }
                }
                if let Some((u, t)) = record {
                    let t = if t == "Đang tải…" || t == "Tab mới" { String::new() } else { t };
                    self.store.visit(&u, &t);
                    self.visit_count += 1;
                    if self.visit_count % 10 == 0 {
                        self.store.save_history();
                    }
                    self.save_session();
                }
                if home {
                    if let Some(tab) = self.tabs.iter().find(|x| x.id == id) {
                        self.push_home(tab);
                    }
                }
                self.push_state();
            }
        }
    }
}

// ---------------------------------------------------------------------------

fn main() {
    // Với GPU đồ hoạ lỗi (màn hình đen/không vẽ được): chạy với
    // AURORA_SAFE_RENDERER=1 để tắt DMA-BUF renderer của WebKit.
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if std::env::var("AURORA_SAFE_RENDERER").as_deref() == Ok("1") {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Aurora")
        .with_inner_size(TaoSize::new(1360.0, 860.0))
        .with_min_inner_size(TaoSize::new(560.0, 380.0))
        .build(&event_loop)
        .expect("không tạo được cửa sổ");

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let fixed = {
        use gtk::prelude::*;
        use tao::platform::unix::WindowExtUnix;
        let fixed = gtk::Fixed::new();
        let vbox = window.default_vbox().expect("không có vbox");
        vbox.pack_start(&fixed, true, true, 0);
        fixed.show_all();
        fixed
    };

    let host = Host {
        window,
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        fixed,
    };

    let store = Store::load();
    let collapsed = store.session.sidebar_collapsed;
    let config = config::shared();

    // Linux: HTTP cục bộ phục vụ video nền (WebKitGTK không phát media qua custom scheme).
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let wall_port = wallhttp::start(config.clone());
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let wall_port: Option<u16> = None;
    let _ = WALL_PORT.set(wall_port);
    if std::env::var("AURORA_DEBUG").is_ok() {
        match wall_port {
            Some(p) => eprintln!("[aurora] wallpaper http server: http://127.0.0.1:{p}/wallpaper"),
            None => eprintln!("[aurora] wallpaper http server: không khởi động được"),
        }
    }

    // Một WebContext duy nhất cho cả trình duyệt:
    // cookie / cache / localStorage dùng chung giữa các tab và lưu bền vững
    // trong thư mục dữ liệu (đăng nhập Google/Microsoft giữ nguyên sau restart).
    let profile = store::dir().join("webdata");
    let _ = std::fs::create_dir_all(&profile);
    let mut web_context = WebContext::new(Some(profile));

    // Tải về lưu vào ~/Downloads.
    let cp_dl = proxy.clone();
    let chrome_page = chrome_html();
    let chrome = host.attach(
        WebViewBuilder::with_web_context(&mut web_context)
            .with_html(&chrome_page)
            .with_ipc_handler(move |req| {
                let _ = cp_dl.send_event(UserEvent::Chrome(req.body().clone()));
            })
            .with_download_started_handler(|_url, path| {
                if let Some(dl) = dirs::download_dir() {
                    let _ = std::fs::create_dir_all(&dl);
                    if let Some(name) = path.file_name() {
                        *path = unique_download_path(dl.join(name));
                    }
                }
                true
            })
            .with_download_completed_handler(|_url, path, ok| {
                if ok {
                    let p = path
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "tệp đã tải".into());
                    println!("[aurora] đã tải xong: {p}");
                }
            }),
        rect(0.0, 0.0, 1360.0, 860.0),
    );

    // Linux: bật favicon database để biểu tượng tab đẹp và chuẩn hơn.
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use webkit2gtk::{WebContextExt, WebViewExt};
        let wk = chrome.webview();
        if let Some(ctx) = wk.context() {
            let fdir = store::dir().join("favicons");
            let _ = std::fs::create_dir_all(&fdir);
            ctx.set_favicon_database_directory(Some(&fdir.to_string_lossy()));
        }
        tune_webview(&chrome, &config.lock().unwrap());
    }

    let overlay = make_overlay(&host, &mut web_context, &proxy);

    let mut app = App {
        host,
        chrome,
        overlay,
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        proxy,
        quit: false,
        store,
        collapsed,
        items: Vec::new(),
        sel: None,
        mode: Mode::Hidden,
        closed: Vec::new(),
        visit_count: 0,
        config,
        web_context,
        wp_out: None,
    };
    app.restore();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                app.persist();
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                ..
            } => app.layout(),
            Event::UserEvent(ev) => {
                app.handle(ev);
                if app.quit {
                    app.persist();
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}
