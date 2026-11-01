//! Máy chủ HTTP cục bộ cho ảnh/video nền (chỉ Linux).
//!
//! WebKitGTK không thể phát media (<video>) trực tiếp từ custom URI scheme —
//! xem bug WebKit https://bugs.webkit.org/show_bug.cgi?id=146351 và các issue
//! tauri-apps/tauri#8654, #3725. Vì vậy Aurora phục vụ tệp nền dạng video qua
//! một HTTP server nhỏ chỉ nghe trên 127.0.0.1, hỗ trợ cả Range (206) để tua
//! video mượt. Ảnh thì vẫn đi đường `aurora://` như thường.

use crate::config::SharedConfig;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};

/// Khởi động server nền, trả về cổng đã cấp.
pub fn start(cfg: SharedConfig) -> Option<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = handle(&mut stream, &cfg);
        }
    });
    Some(port)
}

fn simple(stream: &mut TcpStream, status: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

/// "range: bytes=0-" / "bytes=100-999" => (start, end)
fn parse_range(line: &str, len: u64) -> Option<(u64, u64)> {
    let spec = line.split_once(':')?.1.trim();
    let spec = spec.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.trim().parse().ok()?;
    let end: u64 = b.trim().parse().unwrap_or(len.saturating_sub(1));
    Some((start, end.max(start)))
}

fn handle(stream: &mut TcpStream, cfg: &SharedConfig) -> std::io::Result<()> {
    // Đọc phần đầu request (đến \r\n\r\n).
    let mut head = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        head.extend_from_slice(&chunk[..n]);
        if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > 65536 {
            break;
        }
    }
    let req = String::from_utf8_lossy(&head).to_string();
    let mut parts = req.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method != "GET" && method != "HEAD" {
        return simple(stream, "405 Method Not Allowed", b"chi cho GET/HEAD");
    }
    if !path.starts_with("/wallpaper") {
        return simple(stream, "404 Not Found", b"khong tim thay");
    }

    let c = cfg.lock().unwrap().clone();
    let Some(file_path) = c.wallpaper.clone() else {
        return simple(stream, "404 Not Found", b"chua co nen");
    };
    let Some(mime) = crate::config::wallpaper_mime(&file_path) else {
        return simple(stream, "415 Unsupported Media Type", b"khong ho tro kieu tep");
    };
    let Ok(mut file) = std::fs::File::open(&file_path) else {
        return simple(stream, "404 Not Found", b"khong doc duoc tep nen");
    };
    let len = file.metadata()?.len();
    if len == 0 {
        return simple(stream, "404 Not Found", b"tep rong");
    }

    // Range nếu WebKit yêu cầu (phục vụ tua video).
    let range = req
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("range:"))
        .and_then(|l| parse_range(l, len));
    let (status, start, end, content_range) = match range {
        Some((s, e)) if s < len => (
            "206 Partial Content",
            s,
            e.min(len - 1),
            format!("Content-Range: bytes {s}-{}/{len}\r\n", e.min(len - 1)),
        ),
        _ => ("200 OK", 0u64, len - 1, String::new()),
    };
    let clen = end - start + 1;

    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {clen}\r\nAccept-Ranges: bytes\r\nCache-Control: no-cache\r\n{content_range}Connection: close\r\n\r\n"
    );
    stream.write_all(head.as_bytes())?;
    if method == "GET" {
        file.seek(SeekFrom::Start(start))?;
        let mut left = clen;
        let mut buf = vec![0u8; 256 * 1024];
        while left > 0 {
            let want = buf.len().min(left as usize);
            let n = file.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n])?;
            left -= n as u64;
        }
    }
    Ok(())
}
