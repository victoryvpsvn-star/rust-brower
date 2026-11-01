#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

//! Aurora - trình duyệt Rust (tao + wry).
//!
//! Kiến trúc (lấy ý tưởng từ Zen / Min / servoshell):
//!   - `chrome`  : webview UI (thanh bên dọc + thanh công cụ), nằm dưới cùng.
//!   - mỗi tab   : một webview con, tab chưa dùng được nạp lười (lazy) khi khôi phục phiên.
//!   - `overlay` : webview nổi trên cùng cho gợi ý địa chỉ / lệnh / tìm trong trang.
//!   - Rust giữ toàn bộ trạng thái (tab, bookmark, lịch sử), UI chỉ là bộ vẽ.

mod store;

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
    PageLoadEvent, Rect, WebView, WebViewBuilder,
};

const KEYS: &str = include_str!("keys.js");
const PAGE: &str = include_str!("page.js");
const NEWTAB_HTML: &str = include_str!("newtab.html");

const TOP_H: f64 = 52.0;
const SIDEBAR_W: f64 = 248.0;
const SIDEBAR_C: f64 = 60.0;

const SEARCH: &str = "https://www.google.com/search?q=";
const SEARCH_NAME: &str = "Google";

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
    ("clearhistory", "Xoá lịch sử duyệt web", ""),
];

/// Bang kiểu Min / DuckDuckGo: `!yt rust` => tìm thẳng trên YouTube.
const BANGS: &[(&str, &str)] = &[
    ("g", "https://www.google.com/search?q="),
    ("ddg", "https://duckduckgo.com/?q="),
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

/// Trả về (url, là_tìm_kiếm).
fn resolve(input: &str) -> (String, bool) {
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
        (format!("{}{}", SEARCH, urlencode(t)), true)
    }
}

/// URL nội bộ (trang New Tab) thì coi như rỗng.
fn display_url(u: &str) -> String {
    if u.starts_with("about:") || u.starts_with("data:") || u.starts_with("wry:") || u.contains("wry.localhost") {
        String::new()
    } else {
        u.to_string()
    }
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

/// Overlay luôn phải nằm trên cùng, nên được dựng lại mỗi khi có tab mới.
fn make_overlay(host: &Host, proxy: &EventLoopProxy<UserEvent>) -> WebView {
    let p = proxy.clone();
    host.attach(
        WebViewBuilder::new()
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

        let (p_title, p_load, p_ipc) = (self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let script = page_script();
        let mut b = WebViewBuilder::new()
            .with_devtools(true)
            .with_back_forward_navigation_gestures(true)
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

        b = match (&url, load_now) {
            (Some(u), true) => b.with_url(u),
            _ => b.with_html(NEWTAB_HTML),
        };

        let (w, h) = self.win_size();
        let sw = self.sidebar_w();
        let webview = self.host.attach(b, rect(sw, TOP_H, w - sw, h - TOP_H));
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
        self.overlay = make_overlay(&self.host, &self.proxy);
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
        let (url, _) = resolve(text);
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
            let (url, is_search) = resolve(t);
            if is_search {
                out.push(Item { kind: "search", title: format!("Tìm “{t}”"), sub: SEARCH_NAME.into(), url, action: String::new() });
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

    let cp = proxy.clone();
    let chrome_page = chrome_html();
    let chrome = host.attach(
        WebViewBuilder::new()
            .with_html(&chrome_page)
            .with_ipc_handler(move |req| {
                let _ = cp.send_event(UserEvent::Chrome(req.body().clone()));
            }),
        rect(0.0, 0.0, 1360.0, 860.0),
    );
    let overlay = make_overlay(&host, &proxy);

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
