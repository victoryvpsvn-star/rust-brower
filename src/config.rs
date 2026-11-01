//! Cấu hình người dùng: công cụ tìm kiếm, ảnh nền, tương thích đăng nhập,
//! tăng tốc GPU và khoá cài đặt cho game thủ (Game Mode, FPS overlay,
//! chặn quảng cáo, ngủ tab, màu nhấn giao diện).
//!
//! Lưu dạng JSON (`config.json`) cùng thư mục dữ liệu với bookmark/lich sử.
//! Các trường mới đều có `#[serde(default)]` nên config.json cũ vẫn nạp được.

use crate::store;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Công cụ tìm kiếm: (id, tên hiển thị, URL tìm kiếm kèm tham số).
pub struct Engine {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

/// Google bị giới hạn/throttle khi tra cứu nhiều — để người dùng đổi nhanh
/// sang Bing, DuckDuckGo, Brave hay Yandex chỉ bằng 1 cú bấm trên trang chủ.
pub const ENGINES: &[Engine] = &[
    Engine { id: "google", name: "Google", url: "https://www.google.com/search?q=" },
    Engine { id: "bing", name: "Bing", url: "https://www.bing.com/search?q=" },
    Engine { id: "duckduckgo", name: "DuckDuckGo", url: "https://duckduckgo.com/?q=" },
    Engine { id: "brave", name: "Brave", url: "https://search.brave.com/search?q=" },
    Engine { id: "yandex", name: "Yandex", url: "https://www.yandex.com/search/?text=" },
];

pub fn engine_of(id: &str) -> &'static Engine {
    ENGINES.iter().find(|e| e.id == id).unwrap_or(&ENGINES[0])
}

/// Màu nhấn giao diện (kiểu "Choose a theme color" của Chrome).
/// (id, tên hiển thị, màu 1, màu 2 của gradient)
pub struct Accent {
    pub id: &'static str,
    pub name: &'static str,
    pub c1: &'static str,
    pub c2: &'static str,
}

pub const ACCENTS: &[Accent] = &[
    Accent { id: "violet", name: "Tím Aurora", c1: "#8b7bff", c2: "#5eb8ff" },
    Accent { id: "blue", name: "Xanh Dương", c1: "#5b8cff", c2: "#38c6ff" },
    Accent { id: "green", name: "Xanh Lục", c1: "#37d67a", c2: "#2fd4b0" },
    Accent { id: "rose", name: "Hồng Neon", c1: "#ff5c8a", c2: "#ff8f6b" },
    Accent { id: "amber", name: "Cam Lửa", c1: "#ffb84d", c2: "#ff7a45" },
    Accent { id: "gaming", name: "RGB Game Thủ", c1: "#a3ff12", c2: "#12fff7" },
];

pub fn accent_of(id: &str) -> &'static Accent {
    ACCENTS.iter().find(|a| a.id == id).unwrap_or(&ACCENTS[0])
}

/// User-agent "hình Chrome".
///
/// Google chặn đăng nhập từ WebKitGTK khi UA mặc định ("trình duyệt hoặc ứng
/// dụng này có thể không an toàn"); giả UA Chrome thì đăng nhập Google và
/// Microsoft hoạt động bình thường trên engine WebKit.
pub fn compat_ua() -> String {
    let ua = if cfg!(target_os = "windows") {
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
    } else if cfg!(target_os = "macos") {
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
    } else {
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
    };
    ua.to_string()
}

/// MIME của tệp ảnh/video nền theo phần mở rộng.
pub fn wallpaper_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        "mp4" | "m4v" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "mkv" => Some("video/x-matroska"),
        "mov" => Some("video/quicktime"),
        _ => None,
    }
}

/// Cấu hình dùng chung giữa App và trình xử lý giao thức `aurora://`.
pub type SharedConfig = Arc<Mutex<Config>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// id của công cụ tìm kiếm mặc định (xem `ENGINES`).
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Giả user-agent Chrome để đăng nhập Google/Microsoft trên WebKitGTK.
    #[serde(default = "default_true")]
    pub ua_compat: bool,
    /// Luôn bật tăng tốc GPU (mượt hơn, vẽ nhanh hơn).
    #[serde(default = "default_true")]
    pub gpu_always: bool,
    /// Đường dẫn tuyệt đối tới tệp ảnh/video nền (png, jpg, gif, webp, mp4, webm…).
    #[serde(default)]
    pub wallpaper: Option<String>,
    /// id màu nhấn giao diện (xem `ACCENTS`).
    #[serde(default = "default_accent")]
    pub accent: String,
    /// Hiện khu game trên trang chủ (tiles game/webgame cho game thủ).
    #[serde(default = "default_true")]
    pub gamer_home: bool,
    /// Chặn quảng cáo & theo dõi (Ad Block Lite, chặn ở mức DOM).
    #[serde(default = "default_true")]
    pub adblock: bool,
    /// Ngủ tab không dùng để tiết kiệm RAM (Sleeping Tabs).
    #[serde(default = "default_true")]
    pub sleeping_tabs: bool,
    /// Số phút không dùng thì cho tab ngủ.
    #[serde(default = "default_sleep_min")]
    pub sleep_after_min: u32,
    /// Game Mode: nâng ưu tiên tiến trình, tắt cuộn mượt để giảm độ trễ.
    #[serde(default)]
    pub game_mode: bool,
    /// Tự động bật Game Mode khi vào toàn màn hình (F11/video fullscreen).
    #[serde(default = "default_true")]
    pub auto_game_mode: bool,
    /// Bật đồng hồ FPS nổi (FPS overlay kiểu GeForce Experience).
    #[serde(default)]
    pub fps_overlay: bool,
}

fn default_engine() -> String {
    "google".to_string()
}

fn default_accent() -> String {
    "violet".to_string()
}

fn default_sleep_min() -> u32 {
    20
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            engine: default_engine(),
            ua_compat: true,
            gpu_always: true,
            wallpaper: None,
            accent: default_accent(),
            gamer_home: true,
            adblock: true,
            sleeping_tabs: true,
            sleep_after_min: default_sleep_min(),
            game_mode: false,
            auto_game_mode: true,
            fps_overlay: false,
        }
    }
}

pub fn load() -> Config {
    store::load::<Config>("config.json")
}

pub fn save(c: &Config) {
    store::save("config.json", c);
}

/// Nạp cấu hình và bọc trong `Arc<Mutex<…>>` để handler của giao thức tùy
/// chỉnh luôn đọc giá trị mới nhất mỗi lần có yêu cầu.
pub fn shared() -> SharedConfig {
    Arc::new(Mutex::new(load()))
}
