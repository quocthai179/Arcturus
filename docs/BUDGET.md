# Ngân sách tài nguyên

Đây là mục tiêu thiết kế, không phải số đo. Mọi PR đụng `crates/hub*` hoặc `crates/app` phải đo lại bằng `/budget-check`
và ghi kết quả vào bảng "Lịch sử đo" bên dưới.

## Mục tiêu

| Thành phần | RAM (RSS) | CPU khi rảnh | Ghi chú |
|---|---|---|---|
| Hub, 0 phiên | ≤ 15 MB | ≈ 0% | không timer nào chạy nhanh hơn 30 giây |
| Hub, 20 phiên hoạt động | ≤ 40 MB | < 1% | ring buffer sự kiện trong RAM tối đa 2.000 mục |
| App Lite (tray + popup đóng) | ≤ 60 MB | ≈ 0% | không giữ webview khi popup đóng |
| App Lite (popup mở) | ≤ 150 MB tổng | < 0,5% | |
| Văn phòng 2D mở | + 50–80 MB | < 2% | 15 FPS khi có hoạt động, 0 khi tất cả rảnh hoặc ẩn |
| Độ trễ ingest | p99 ≤ 5 ms | | `/hooks/event` trả 204 ngay, xử lý sau |

## Cách đo

Linux/macOS:
```bash
pgrep -f 'target/.*/hub' | xargs -I{} ps -o pid,rss,%cpu,etime,command -p {}
# RSS tính bằng KB
```

Windows (PowerShell):
```powershell
Get-Process hub, arcturus | Select-Object Name, Id, @{n='RSS_MB';e={[math]::Round($_.WorkingSet64/1MB,1)}}, CPU
```

Kịch bản chuẩn:
1. Khởi động hub, chờ 60 giây, đo (idle).
2. `cargo run -p replay -- fixtures/hooks --sessions 20 --rate 50/s --duration 120s`, đo trong lúc chạy.
3. Dừng replay, chờ 60 giây, đo lại. RSS phải quay về gần mức idle (không rò rỉ).

Đo trên ít nhất một máy yếu (ví dụ 4 GB RAM, 2 nhân) trước mỗi mốc phát hành.

## Lịch sử đo

| Ngày | Commit | Máy | Hub idle | Hub 20 phiên | App Lite | 2D | Ghi chú |
|---|---|---|---|---|---|---|---|
| 2026-10-21 | a5fd9f5 | i5-12400F 6C/12T, 32 GB, Windows 11 Pro | 8,3 MB WS / 1,5 MB private, CPU 0,00% | chưa đo (không có replay); tải curl ~22 req/s: đỉnh 8,7 MB WS / 2,1 MB private, CPU ~0,2%; sau nghỉ 60 s 8,7 MB, CPU 0% | chưa đo | chưa đo | T1 record mode, kịch bản rút gọn (chưa có replay). Ingest curl `time_total`: p50 1,8 ms, p99 18 ms (mỗi request một TCP mới + ghi file trước khi trả, chưa đại diện cho p99 ≤ 5 ms) |
| 2026-10-22 | 8f89d40 | i5-12400F 6C/12T, 32 GB, Windows 11 Pro | 8,7 MB WS / 1,5 MB private, CPU 0,000% (12 mẫu/60 s) | 20 `/hooks/permission` giữ đồng thời (20 session_id) + `/hooks/event` ~31 req/s keep-alive (3.760 sự kiện, mọi loại fixture) trong 120 s: đỉnh 9,9 MB WS / 3,3 MB private, CPU TB 1,29% một nhân (0,11% tổng); sau nghỉ 60 s 9,4 MB WS / 2,9 MB private, phẳng, CPU 0% | chưa đo | chưa đo | T3 luồng duyệt, kịch bản xấp xỉ (chưa có replay/WebSocket/SQLite). Ingest p50 0,94 ms, p99 1,46 ms (client Node keep-alive). 20 request giữ đều trả `200 {}` khi hết giờ 110 s, khi `--approval-timeout 5` (5,03 s) và khi Ctrl-C (`shutdown` n=20, tắt ngay, rc=0); client đóng kết nối thì hub hủy yêu cầu (20/20). Cùng kịch bản ở 7539f0d: idle 7,2 MB WS / 1,6 MB private, đỉnh 8,8 / 3,5 MB, sau nghỉ 8,3 / 2,9 MB |
