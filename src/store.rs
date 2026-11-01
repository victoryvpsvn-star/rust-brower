//! Lưu trữ bền vững: bookmark, lịch sử, phiên làm việc (JSON trong thư mục dữ liệu người dùng).

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Thư mục dữ liệu của aurora (dùng chung cho bookmark, lịch sử, phiên, cấu hình, webdata).
pub(crate) fn dir() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("aurora");
    let _ = fs::create_dir_all(&p);
    p
}

pub(crate) fn load<T: for<'de> Deserialize<'de> + Default>(name: &str) -> T {
    fs::read_to_string(dir().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save<T: Serialize>(name: &str, v: &T) {
    if let Ok(s) = serde_json::to_string(v) {
        let tmp = dir().join(format!("{name}.tmp"));
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, dir().join(name));
        }
    }
}

pub fn host_of(url: &str) -> String {
    let s = url.split("://").nth(1).unwrap_or(url);
    s.split('/')
        .next()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_string()
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub visits: u32,
    pub last: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SavedTab {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Session {
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub sidebar_collapsed: bool,
}

pub struct Hit {
    pub kind: &'static str,
    pub title: String,
    pub url: String,
}

pub struct Store {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<Visit>,
    pub session: Session,
}

fn frecency(v: &Visit, now: u64) -> f64 {
    let age_days = now.saturating_sub(v.last) as f64 / 86_400.0;
    v.visits as f64 / (1.0 + age_days / 14.0)
}

impl Store {
    pub fn load() -> Self {
        Store {
            bookmarks: load("bookmarks.json"),
            history: load("history.json"),
            session: load("session.json"),
        }
    }

    pub fn save_bookmarks(&self) {
        save("bookmarks.json", &self.bookmarks);
    }
    pub fn save_history(&self) {
        save("history.json", &self.history);
    }
    pub fn save_session(&self) {
        save("session.json", &self.session);
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        !url.is_empty() && self.bookmarks.iter().any(|b| b.url == url)
    }

    /// Trả về true nếu sau khi gọi, trang đang được đánh dấu.
    pub fn toggle_bookmark(&mut self, url: &str, title: &str) -> bool {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
            self.save_bookmarks();
            false
        } else {
            self.bookmarks.push(Bookmark { url: url.to_string(), title: title.to_string() });
            self.save_bookmarks();
            true
        }
    }

    pub fn visit(&mut self, url: &str, title: &str) {
        let t = now();
        if let Some(v) = self.history.iter_mut().find(|v| v.url == url) {
            v.visits += 1;
            v.last = t;
            if !title.is_empty() {
                v.title = title.to_string();
            }
        } else {
            self.history.push(Visit { url: url.to_string(), title: title.to_string(), visits: 1, last: t });
        }
        if self.history.len() > 5000 {
            self.history.sort_by(|a, b| b.last.cmp(&a.last));
            self.history.truncate(4000);
        }
    }

    pub fn set_title(&mut self, url: &str, title: &str) {
        if title.is_empty() {
            return;
        }
        if let Some(v) = self.history.iter_mut().find(|v| v.url == url) {
            v.title = title.to_string();
        }
    }

    /// Các trang hay vào nhất, mỗi tên miền một mục.
    pub fn top_sites(&self, n: usize) -> Vec<Visit> {
        let t = now();
        let mut all: Vec<&Visit> = self.history.iter().collect();
        all.sort_by(|a, b| {
            frecency(b, t)
                .partial_cmp(&frecency(a, t))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for v in all {
            if seen.insert(host_of(&v.url)) {
                out.push(v.clone());
                if out.len() >= n {
                    break;
                }
            }
        }
        out
    }

    /// Gợi ý theo kiểu Min: khớp mọi từ khoá trong tiêu đề + URL, ưu tiên bookmark và trang hay vào.
    pub fn suggest(&self, q: &str, limit: usize) -> Vec<Hit> {
        let t = now();
        let toks: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();

        if toks.is_empty() {
            return self
                .top_sites(limit)
                .into_iter()
                .map(|v| Hit { kind: "history", title: v.title, url: v.url })
                .collect();
        }

        let matches = |hay: &str| toks.iter().all(|k| hay.contains(k.as_str()));
        let host_bonus = |url: &str| {
            let h = host_of(url).to_lowercase();
            if toks.iter().any(|k| h.starts_with(k.as_str())) {
                50.0
            } else {
                0.0
            }
        };

        let mut scored: Vec<(f64, Hit)> = Vec::new();
        for b in &self.bookmarks {
            let hay = format!("{} {}", b.title, b.url).to_lowercase();
            if matches(&hay) {
                scored.push((1000.0 + host_bonus(&b.url), Hit { kind: "bookmark", title: b.title.clone(), url: b.url.clone() }));
            }
        }
        for v in &self.history {
            let hay = format!("{} {}", v.title, v.url).to_lowercase();
            if matches(&hay) {
                scored.push((frecency(v, t) + host_bonus(&v.url), Hit { kind: "history", title: v.title.clone(), url: v.url.clone() }));
            }
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (_, h) in scored {
            if seen.insert(h.url.clone()) {
                out.push(h);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }
}
