# Bảng task — Arcturus v0

YouTrack: https://izar.youtrack.cloud/issues/ARC — Epic (Milestone) → Story (Task) → Dev task (Subtask), phụ thuộc bằng link "depends on".
Epic v0: ARC-1 Nền móng · ARC-2 Hub và hàng đợi duyệt · ARC-3 Policy · ARC-4 Giao diện · ARC-5 Đo lường và dogfood.
Epic sau v0: ARC-6 (v1 VRM, giọng nói, ủy quyền 7 mức) · ARC-7 (v2 đa agent, đa máy, mobile). Thiết bị ESP32 là sản phẩm riêng: project IZAR (IZAR-8).

Trạng thái: `todo` · `doing` · `review` · `done` · `blocked`
Làn: `main` (giai đoạn A, một phiên) · `hub` · `policy` · `ui` · `lab`
Ước lượng tính bằng buổi tối (khoảng 2–3 giờ).

## Giai đoạn A — nền móng (một phiên, tuần 1)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T0 · ARC-21 | main | Scaffold: Cargo workspace (`hub-core`, `hub`, `replay`), Tauri 2 trong `crates/app`, `ui/` Vite+React+TS, CI chạy fmt/clippy/test | — | 2 | todo |
| T1 · ARC-22 | main | Hook lab: hub "ghi băng" tối giản nhận mọi hook và lưu JSON thô; ghi fixture từ 3–4 phiên thật; trả lời Q1–Q4 trong HOOK_FINDINGS.md | T0 | 2 | todo |
| T2 · ARC-23 | main | Chốt `schema.rs` (Event v1) + chuẩn hóa từ JSON hook, test với toàn bộ fixture | T1 | 1 | todo |

**Tiêu chí chấp nhận**
- T0: `cargo test --workspace`, `pnpm -C ui test`, `pnpm -C crates/app tauri dev` đều chạy được; CI xanh.
- T1: ≥30 fixture bao phủ SessionStart, PreToolUse (Edit/Read/Bash), PermissionRequest, Stop, SubagentStart/Stop; Q1–Q4 có kết quả kèm bằng chứng.
- T2: mọi fixture parse thành Event không lỗi; có test snapshot.

## Giai đoạn B — các làn song song (tuần 2–5)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T3 · ARC-24 | hub | Luồng duyệt: `/hooks/permission` giữ request bằng oneshot, timeout 110 s trả `{}`, hủy khi SessionEnd | T2 | 3 | todo |
| T4 · ARC-25 | hub | State machine agent theo bảng trong ARCHITECTURE.md; persona từ `arcturus.toml` | T2 | 2 | todo |
| T5 · ARC-26 | hub | WebSocket `/ws`: snapshot + delta; viết `docs/PROTOCOL.md`; xác thực token | T3, T4 | 2 | todo |
| T6 · ARC-27 | hub | SQLite WAL: events (TTL 30 ngày), approvals; ring buffer trong RAM | T2 | 2 | todo |
| T7 · ARC-28 | hub | Đối soát `claude agents --json` mỗi 30 s, dọn phiên chết | T4 | 1 | todo |
| T8 · ARC-29 | policy | Danger list + chuẩn hóa lệnh (bỏ ký tự ẩn/bidi), tính `risk` | T2 | 2 | todo |
| T9 · ARC-30 | policy | Batch rule ("cho phép mẫu X trong N phút") + gom yêu cầu giống nhau | T8 | 2 | todo |
| T10 · ARC-31 | policy | Trust grant cơ bản: đếm chuỗi duyệt liên tiếp, đề xuất nâng mức, tự hạ khi bị deny/hoàn tác | T8, T6 | 3 | todo |
| T11 · ARC-32 | ui | Component nhân vật SVG (mood × palette), port từ mockup | T0 | 1 | todo |
| T12 · ARC-33 | ui | Tray + badge số người giơ tay; popup Lite (hàng đợi, duyệt/từ chối, duyệt gộp) | T5, T11 | 3 | todo |
| T13 · ARC-34 | ui | Văn phòng 2D: lưới bàn, trạng thái, "phòng hỗ trợ" gom agent rảnh (đội hình chính ≤5) | T5, T11 | 3 | todo |
| T14 · ARC-35 | ui | Nút "Tạm dừng cả đội", "Giờ tập trung"; đếm ngược phủ quyết cho yêu cầu đảo ngược được | T12 | 2 | todo |
| T15 · ARC-36 | lab | `tools/replay`: phát lại fixture với số phiên/tốc độ tùy chọn | T2 | 1 | todo |
| T16 · ARC-37 | lab | Kịch bản đo ngân sách + bảng lịch sử trong BUDGET.md; chạy trên máy yếu | T15, T5 | 1 | todo |
| T17 · ARC-38 | lab | Test e2e: Claude Code thật (headless `-p`) + hub, kiểm tra allow/deny/timeout | T3 | 2 | todo |

**Tiêu chí chấp nhận chính**
- T3: test cho allow, deny, timeout trả `{}`, phiên kết thúc khi đang chờ, hub nhận 2 yêu cầu cùng lúc từ 2 phiên.
- T8: danger list có test cho từng mẫu (kể cả biến thể như `rm -fr`, `git push -f`, có khoảng trắng/ký tự ẩn); không mẫu nào trong danh sách có thể bị tự allow.
- T12: popup mở ≤300 ms; đóng popup thì webview bị hủy (RSS app giảm về mức Lite).
- T13: tab ẩn thì không còn frame nào được vẽ (kiểm bằng performance profiler).

## Giai đoạn C — dogfood và mốc v0 (tuần 6)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T18 · ARC-39 | lab | `hooks/arcturus-dogfood.json` + hướng dẫn cài/gỡ; bật dogfood | T5, T12 | 1 | todo |
| T19 · ARC-40 | hub | Standup sinh từ template (sự kiện + git log), 0 token | T6 | 2 | todo |
| T20 · ARC-41 | lab | Đo chỉ số: thời gian xử lý trung bình mỗi yêu cầu, thời gian agent tự chạy; so trước/sau | T18 | 1 | todo |
| T21 · ARC-42 | main | `/milestone-gate v0` | tất cả | 1 | todo |

## Blocked

<!-- ID · làn · bị chặn bởi gì · cần ai quyết -->

## Dogfood

<!-- Ghi những chỗ khó chịu khi dùng thật -->

## Nhật ký

<!-- YYYY-MM-DD · làn · xong gì · vướng gì -->
- 2026-10-21 · main · Đồng bộ tài liệu (nhánh `docs-sync-contracts`, chưa có ticket ARC): xóa `PLAN.md` trùng ở gốc; ARCHITECTURE.md khớp PLAN.md §3 (Event, SQLite, tên thông điệp WS, thứ tự policy, glob persona); T3 tạm dừng tắt AutoAllow trước khi trả allow; `danger_spans` theo chỉ số ký tự Unicode (§3.4, T8, T12); README/WORKFLOW bỏ cách nói "chép bộ quy trình". · Vướng: chưa có ticket ARC cho việc này.
