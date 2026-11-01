# Aurora

Trình duyệt viết bằng Rust (tao + wry), giao diện thanh tab dọc kiểu Zen, thanh lệnh kiểu Min.

**Bản 0.4 — "siêu bá vương cho game thủ"**: Cài đặt kiểu Google Chrome, Chế độ Game, đồng hồ FPS, chặn quảng cáo, ngủ tab tiết kiệm RAM.

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

## Có gì mới trong 0.4

### 🎮 Chế độ Game (Game Mode)
- **F9** bật/tắt ngay, hoặc nút gamepad trên thanh công cụ.
- Nâng ưu tiên tiến trình trình duyệt (nice -10 trên Linux), tắt cuộn mượt để **giảm độ trễ chuột → khung hình**.
- **Tự bật khi vào toàn màn hình (F11)**, ra ngoài tự tắt (bật/tắt được trong Cài đặt).

### 🎯 Đồng hồ FPS (FPS overlay)
- Số FPS lớn nổi ở góc phải màn hình (kiểu GeForce Experience), đổi màu theo chất lượng: xanh ≥110, xanh dương ≥55, vàng ≥28, đỏ <28.
- Đo bằng `requestAnimationFrame` ngay trong trang — đúng FPS render thật của game/web.
- Có thêm badge FPS nhỏ trên thanh công cụ.

### 🛡 Ad Block Lite
- Ẩn quảng cáo & trình theo dõi (DoubleClick, Taboola, Outbrain, Analytics, PopAds…) ở mức DOM.
- Đếm số phần tử đã chặn, hiện trên thanh bên ("🛡 n quảng cáo đã chặn").

### 😴 Ngủ tab (Sleeping Tabs)
- Tab không dùng quá N phút (chọn 5–120 phút) tự "ngủ": trang gốc được thay bằng trang siêu nhẹ, **giải phóng RAM**, bấm vào là tỉnh lại ngay tại chỗ cũ.
- Tab đang ghim không bao giờ ngủ.

### ⚙️ Cài đặt kiểu Google Chrome
- Cột điều hướng trái (Bạn và Aurora, Giao diện, Công cụ tìm kiếm, Quyền riêng tư và bảo mật, Hiệu năng, Trò chơi, Hệ thống, Đặt lại cài đặt).
- Ô **"Tìm kiếm trong Cài đặt"** lọc theo từ khoá như Chrome.
- Công tắc toggle Material, **6 màu nhấn giao diện** (kể cả RGB Game Thủ) đổi là áp dụng tức thì.
- Nút đặt lại cài đặt về mặc định, mở thư mục dữ liệu, xoá lịch sử.

### Khác
- **Khu game thủ trên trang chủ**: Krunker, Shell Shockers, Slither.io, 2048, Chess.com, Poki, CrazyGames, Steam, Epic, Discord, Twitch (tắt được trong Cài đặt).
- Thanh bên hiển thị **RAM hệ thống (%) + số quảng cáo đã chặn**.
- Bang mới: `!st` (Steam), `!tw` (Twitch).

## Giữ từ bản 0.3

- **Đăng nhập Google / Microsoft**: một `WebContext` dùng chung cho mọi tab, cookie/cache/localStorage **lưu bền vững** — tắt mở lại vẫn còn đăng nhập. User-agent "hình Chrome" để Google không chặn.
- **Ảnh nền động** (.png, .jpg, .gif, .webp, .mp4, .webm…) qua `aurora://` + HTTP cục bộ (Range/206) cho video.
- **Đổi công cụ tìm kiếm nhanh** (Google / Bing / DuckDuckGo / Brave / Yandex) + bang `!g !b !ddg !yt !gh !w !so !crates !docs !st !tw`.
- WebRTC + MediaStream cho Google Meet/Discord, favicon database, download vào `~/Downloads`.
- GPU lỗi/màn hình đen? `AURORA_SAFE_RENDERER=1 aurora`.

## Build tự động (GitHub Actions)

Đẩy code lên GitHub, workflow `.github/workflows/build.yml` tự build cho Linux, Windows, macOS. File build xuất hiện ở tab **Actions → Artifacts**. Gắn tag `v*` (ví dụ `git tag v0.4.0 && git push --tags`) để tự tạo bản **Release** kèm file tải về.

## Phím tắt

| Phím | Việc |
|---|---|
| **F9** | **Bật/tắt Chế độ Game** |
| **F11** | **Toàn màn hình** (tự bật Game Mode nếu cấu hình cho phép) |
| Ctrl+T / Ctrl+W / Ctrl+Shift+T | Tab mới / đóng / mở lại tab đã đóng |
| Ctrl+L hoặc Ctrl+K | Thanh địa chỉ |
| Ctrl+F | Tìm trong trang |
| Ctrl+D | Đánh dấu trang |
| Ctrl+B | Thu gọn thanh bên |
| Ctrl + / - / 0 | Zoom |
| Ctrl+Tab, Ctrl+1..9 | Chuyển tab |
| Alt+← / Alt+→ | Quay lại / tiến tới |
| F12 | Công cụ nhà phát triển |

Trong thanh địa chỉ: gõ `>` để xem lệnh, `!g` `!b` `!ddg` `!yt` `!st` `!tw` `!gh` `!w` `!so` `!crates` `!docs` kèm từ khoá để tìm nhanh.

## Dữ liệu người dùng

Bookmark, lịch sử, phiên làm việc, cấu hình và **dữ liệu web (cookie/đăng nhập/cache)** nằm trong thư mục `aurora` của thư mục dữ liệu người dùng (Linux: `~/.local/share/aurora`). Mở nhanh từ **Cài đặt → Bạn và Aurora → Mở thư mục**.

## Gỡ lỗi

Chạy với `AURORA_DEBUG=1` để xem mỗi yêu cầu `aurora://` trên stderr (hữu ích khi kiểm tra ảnh nền).
