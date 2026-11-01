# Aurora

Trình duyệt viết bằng Rust (tao + wry), giao diện thanh tab dọc kiểu Zen, thanh lệnh kiểu Min.

## Chạy thử

```bash
cargo run --release
```

Linux cần cài trước:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libxdo-dev pkg-config
```

Windows cần WebView2 (Windows 10/11 thường có sẵn). macOS không cần thêm gì.

## Build tự động (GitHub Actions)

Đẩy code lên GitHub, workflow `.github/workflows/build.yml` tự build cho Linux, Windows, macOS.
File build xuất hiện ở tab **Actions → Artifacts**. Gắn tag `v*` (ví dụ `git tag v0.2.0 && git push --tags`)
để tự tạo bản **Release** kèm file tải về.

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

Trong thanh địa chỉ: gõ `>` để xem lệnh, `!g` `!yt` `!gh` `!w` `!so` `!crates` `!docs` kèm từ khoá để tìm nhanh.

## Dữ liệu

Bookmark, lịch sử và phiên làm việc lưu dạng JSON trong thư mục `aurora` của thư mục dữ liệu người dùng.
