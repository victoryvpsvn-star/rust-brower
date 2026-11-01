//! Cấu hình người dùng: công cụ tìm kiếm, ảnh nền, tương thích đăng nhập, tăng tốc GPU.
//!
//! Lưu dạng JSON (`config.json`) cùng thư mục dữ liệu với bookmark/lich sử.

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
}

fn default_engine() -> String {
    "google".to_string()
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
