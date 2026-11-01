# Aurora

Trình duyệt viết bằng Rust (tao + wry), giao diện thanh tab dọc kiểu Zen, thanh lệnh kiểu Min.

**Bản 0.3** — sửa lỗi build Linux, đăng nhập Google/Microsoft, ảnh nền động, đổi công cụ tìm kiếm, tăng tốc GPU.

## Chạy thử

```bash
cargo run --release
```

Linux cần cài trước:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libxdo-dev pkg-config
# để phát video nền .mp4 (H.264) nếu chưa có:
sudo apt install gstreamer1.0-libav gstreamer1.0-plugins-bad gstreamer1.0-plugins-good gstreamer1.0-plugins-ugly
```

Windows cần WebView2 (Windows 10/11 thường có sẵn). macOS không cần thêm gì.

## Có gì mới trong 0.3

### Sửa lỗi build
`error[E0599]: no method named build_gtk` — thiếu `use wry::WebViewBuilderExtUnix;` (trait chứa `build_gtk` trên Linux). Đã fix, kèm `WebViewExtUnix` để canh chỉnh WebKitGTK.

### Đăng nhập Google / Microsoft
- **Một `WebContext` dùng chung cho mọi tab** (bản trước mỗi tab một context — cookie không chia sẻ!). Giờ cookie, bộ nhớ đệm, localStorage… dùng chung và **lưu bền vững** trong thư mục dữ liệu: tắt mở lại trình duyệt vẫn còn đăng nhập.
- **User-agent "hình Chrome"** (bật mặc định, tắt được trong Cài đặt): Google chặn đăng nhập từ UA WebKitGTK mặc định ("trình duyệt này có thể không an toàn"), UA Chrome thì đăng nhập bình thường.
- WebRTC + MediaStream bật sẵn cho Google Meet/Discord.

### Ảnh nền tùy thích (.png, .jpg, .gif, .webp, .mp4, .webm…)
- Vào **Cài đặt → Ảnh nền → Chọn tệp…** (hoặc bấm "Đổi nền" ở góc dưới phải trang chủ).
- Ảnh gif và **video động (.mp4/.webm) chạy ngay trên trang chủ**.
- Ảnh đi qua giao thức nội bộ `aurora://`; video đi qua một HTTP server cục bộ (127.0.0.1, hỗ trợ Range/206) vì WebKitGTK không phát media trực tiếp từ custom scheme (bug.webkit.org #146351).
- Nền lưu trong `config.json`, tự áp dụng lại lần sau.

### Đổi công cụ tìm kiếm nhanh
Google bị giới hạn/đổi kết quả? Bấm chọn **Google / Bing / DuckDuckGo / Brave / Yandex** ngay dưới ô tìm kiếm của trang chủ. Gõ `!g`, `!ddg`, `!b`, `!yt`… trước từ khoá để tìm từng trang.

### Nhẹ, mượt, tăng FPS
- Chính sách **"luôn dùng GPU"** (hardware-acceleration-policy = Always), bật trong Cài đặt → Tương thích & hiệu năng.
- **Cuộn mượt** (smooth scrolling), WebRTC/Media bật, **favicon database** cho tab đẹp hơn.
- `WebContext` chung ⇒ ít tiến trình WebKit hơn ⇒ nhẹ RAM hơn.
- Download lưu thẳng vào `~/Downloads`, tự đổi tên khi trùng.
- GPU lỗi/màn hình đen? Chạy `AURORA_SAFE_RENDERER=1 aurora` để tắt DMA-BUF renderer.

## Build tự động (GitHub Actions)

Đẩy code lên GitHub, workflow `.github/workflows/build.yml` tự build cho Linux, Windows, macOS. File build xuất hiện ở tab **Actions → Artifacts**. Gắn tag `v*` (ví dụ `git tag v0.3.0 && git push --tags`) để tự tạo bản **Release** kèm file tải về.

## Phím tắt

| Phím | Việc |
|---|---|
| Ctrl+T / Ctrl+W / Ctrl+Shift+T | Tab mới / đóng / mở lại tab đã đóng |
| Ctrl+L hoặc Ctrl+K | Thanh địa chỉ |
| Ctrl+F | Tìm trong trang |
| Ctrl+D | Đánh dấu trang |
| Ctrl+B | Thu gọn thanh bên |
| Ctrl + / - / 0 | Zoom |
| Ctrl+Tab, Ctrl+1..9 | Chuyển tab |
| Alt+← / Alt+→ | Quay lại / tiến tới |
| F12 | Công cụ nhà phát triển |

Trong thanh địa chỉ: gõ `>` để xem lệnh, `!g` `!b` `!ddg` `!yt` `!gh` `!w` `!so` `!crates` `!docs` kèm từ khoá để tìm nhanh.

## Dữ liệu người dùng

Bookmark, lịch sử, phiên làm việc, cấu hình và **dữ liệu web (cookie/đăng nhập/cache)** nằm trong thư mục `aurora` của thư mục dữ liệu người dùng (Linux: `~/.local/share/aurora`).

## Gỡ lỗi

Chạy với `AURORA_DEBUG=1` để xem mỗi yêu cầu `aurora://` trên stderr (hữu ích khi kiểm tra ảnh nền).
