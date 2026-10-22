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
| T0 · ARC-21 | main | Scaffold: Cargo workspace (`hub-core`, `hub`, `replay`), Tauri 2 trong `crates/app`, `ui/` Vite+React+TS, CI chạy fmt/clippy/test | — | 2 | review |
| T1 · ARC-22 | main | Hook lab: hub "ghi băng" tối giản nhận mọi hook và lưu JSON thô; ghi fixture từ 3–4 phiên thật; trả lời Q1–Q4 trong HOOK_FINDINGS.md | T0 | 2 | review |
| T2 · ARC-23 | main | Chốt `schema.rs` (Event v1) + chuẩn hóa từ JSON hook, test với toàn bộ fixture | T1 | 1 | review |

**Tiêu chí chấp nhận**
- T0: `cargo test --workspace`, `pnpm -C ui test`, `pnpm -C crates/app tauri dev` đều chạy được; CI xanh.
- T1: ≥30 fixture bao phủ SessionStart, PreToolUse (Edit/Read/Bash), PermissionRequest, Stop, SubagentStart/Stop; Q1–Q4 có kết quả kèm bằng chứng.
- T2: mọi fixture parse thành Event không lỗi; có test snapshot.

## Giai đoạn B — các làn song song (tuần 2–5)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T3 · ARC-24 | hub | Luồng duyệt: `/hooks/permission` giữ request bằng oneshot, timeout 110 s trả `{}`, hủy khi SessionEnd | T2 | 3 | review |
| T4 · ARC-25 | hub | State machine agent theo bảng trong ARCHITECTURE.md; persona từ `arcturus.toml` | T2 | 2 | todo |
| T5 · ARC-26 | hub | WebSocket `/ws`: snapshot + delta; viết `docs/PROTOCOL.md`; xác thực token; xử lý `TeamPause`/`TeamResume` phía hub; `GET /approvals/{id}/input` | T3, T4 | 2 | todo |
| T6 · ARC-27 | hub | SQLite WAL: events (TTL 30 ngày), approvals; ring buffer trong RAM | T2 | 2 | todo |
| T7 · ARC-28 | hub | Đối soát `claude agents --json` mỗi 30 s, dọn phiên chết | T4 | 1 | todo |
| T8 · ARC-29 | policy | Danger list + chuẩn hóa lệnh (bỏ ký tự ẩn/bidi), tính `risk` | T2 | 2 | todo |
| T9 · ARC-30 | policy | Batch rule ("cho phép mẫu X trong N phút") + gom yêu cầu giống nhau | T8 | 2 | todo |
| T10 · ARC-31 | policy | Trust grant cơ bản: đếm chuỗi duyệt liên tiếp, đề xuất nâng mức, tự hạ khi bị deny/hoàn tác | T8, T6 | 3 | todo |
| T11 · ARC-32 | ui | Component nhân vật SVG (mood × palette), port từ mockup | T0 | 1 | todo |
| T12 · ARC-33 | ui | Tray + badge số người giơ tay; popup Lite (hàng đợi, duyệt/từ chối, duyệt gộp) | T5, T11 | 3 | todo |
| T13 · ARC-34 | ui | Văn phòng 2D: lưới bàn, trạng thái, "phòng hỗ trợ" gom agent rảnh (đội hình chính ≤5) | T5, T11 | 3 | todo |
| T14 · ARC-35 | ui | Nút "Tạm dừng cả đội" (chỉ UI, hub ở T5), "Giờ tập trung"; đếm ngược phủ quyết cho yêu cầu đảo ngược được | T12 | 2 | todo |
| T15 · ARC-36 | lab | `tools/replay`: phát lại fixture với số phiên/tốc độ tùy chọn | T2 | 1 | todo |
| T16 · ARC-37 | lab | Kịch bản đo ngân sách + bảng lịch sử trong BUDGET.md; chạy trên máy yếu | T15, T5 | 1 | todo |
| T17 · ARC-38 | lab | Test e2e: Claude Code thật (headless `-p`) + hub, kiểm tra allow/deny/timeout | T3 | 2 | todo |

**Tiêu chí chấp nhận chính**
- T3: test cho allow, deny, timeout trả `{}`, phiên kết thúc khi đang chờ, hub nhận 2 yêu cầu cùng lúc từ 2 phiên.
- T8: danger list có test cho từng mẫu (kể cả biến thể như `rm -fr`, `git push -f`, có khoảng trắng/ký tự ẩn); không mẫu nào trong danh sách có thể bị tự allow.
- T12: popup mở ≤300 ms; đóng popup thì webview bị hủy (RSS app giảm về mức Lite).
- T13: tab ẩn thì không còn frame nào được vẽ (kiểm bằng performance profiler).

**Ghi chú từ T1** (bằng chứng ở `docs/HOOK_FINDINGS.md`)
- T2: tên trường subagent là `agent_id` + `agent_type` (Q4); `PermissionRequest` không có `tool_use_id` (Q3).
- T3: terminal hiện hộp thoại **song song** với hub (Q1), nên người dùng có thể trả lời ở terminal trước. Cần cách dọn
  yêu cầu đã được trả lời ở nơi khác (ví dụ sự kiện kế tiếp của cùng `session_id` + `agent_id` thì hủy pending) và thử
  Q1b (Claude Code có đóng kết nối HTTP không).
- T4: hook http cho `SessionStart` không bắn (F1): tạo agent từ sự kiện đầu tiên của một `session_id` lạ, không chờ
  SessionStart.
- T5: audit T1 (finding 8): `/ws?token=` không có CORS bảo vệ → bắt buộc allowlist `Origin` (origin của Tauri) và `Host`
  (`127.0.0.1:<cổng>`, `localhost:<cổng>`), không log query token. Cân nhắc `subtle` cho so sánh token (finding 9).
- T3/T15: p99 ingest đo ở T1 là 18 ms (curl, mỗi request một kết nối TCP mới, ghi file trước khi trả). `/hooks/event`
  thật phải trả 204 trước khi đụng đĩa/SQLite; đo lại bằng `replay` có keep-alive (`docs/BUDGET.md`).
- T12: Q1 = không treo → không cần nút "Trả về terminal" theo điều kiện ở PLAN T12; thẻ duyệt nên ghi rõ "cũng đang hỏi
  trong terminal".
- T18: `hooks/arcturus-dogfood.json` phải đổi `SessionStart` sang hook `type: "command"` gọi `curl` (F1); hook http
  hiện tại sẽ lặng lẽ không gửi gì. Chế độ `auto` không sinh `PermissionRequest` (F2).

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
- 2026-10-21 · main · Chốt 7 câu hỏi thiết kế (người duyệt): bỏ `raw_ref`; tự allow cần ack ghi store; pause ở T5; cắt nội dung Edit/Write + `GET /approvals/{id}/input`; nhãn ký tự ẩn; `match_cwd` trên Windows; tạm dừng tắt đếm ngược, `Decide.origin`, `DecidedBy::Countdown`. · Vướng: không.
- 2026-10-21 · main · T0/ARC-21 scaffold (nhánh `arc-21-scaffold`): workspace Cargo (hub-core, hub, replay) + 4 test; `ui/` React 19, Vite 8, TS 7 strict, Vitest 5 + 1 test; Tauri 2.12 trong `crates/app` (workspace Cargo riêng, mở cửa sổ `ui/` qua Vite); CI GitHub Actions; `.gitattributes` ép LF. Mọi lệnh trong CLAUDE.md chạy được trên Windows. · Vướng: CI chưa chạy (chưa push); app không build trên CI (thiếu webkit trên Ubuntu); `hub`/`replay` mới là khung (T1, T15); chưa đo ngân sách vì hub chưa có server.
- 2026-10-22 · main · T1/ARC-22 hook lab (nhánh `arc-22-hook-lab`, tách từ `arc-21-scaffold`): `hub --record`, `replay scrub`; 64 fixture thật đã scrub; Q1–Q4 có kết quả (Q1 không treo, Q4 có `agent_id`); phát hiện F1 hook http không bắn cho SessionStart; security audit: không có mức cao, đã sửa finding 1–6; budget: hub idle 8,3 MB, CPU 0%. 43 test xanh. · Vướng: Q1b (trả lời ở terminal khi hub đang giữ) chưa thử; p99 ingest 18 ms đo bằng curl chưa đại diện; cần duyệt đề xuất sửa CLAUDE.md "Kiến thức đã kiểm chứng".
- 2026-10-22 · main · T2/ARC-23 schema Event v1 (nhánh `arc-23-schema`): `hub-core/src/schema.rs` theo PLAN §3.1 (không `raw_ref`, có `DecidedBy::Countdown`), `hook.rs` (HookInput chịu lỗi, `to_event`, `tool_summary`, `activity_for_tool`); 64/64 fixture parse thành Event, 11 snapshot insta; PLAN §3.1/§3.2 cập nhật 4 chỗ lệch đã duyệt (agent_id/agent_type, approval do T3 điền, summary ≤200 ký tự, mọi sự kiện có agent_id là agent con). 62 test xanh. · Vướng: không. Chưa đo ngân sách (hub chưa dùng hub-core lúc chạy).
- 2026-10-22 · hub · T3/ARC-24 luồng duyệt (nhánh `arc-24-approval-flow`, commit `7539f0d`, `8f89d40`): `hub/src/approvals.rs` (giữ request bằng oneshot, `response_for` là chỗ duy nhất sinh allow, timeout 110 s/hủy/phiên kết thúc/đóng kết nối/tắt hub đều trả `{}`, đếm ngược và tạm dừng theo §3.4, giới hạn 64/16 pending), `hub/src/hooks.rs` (`/hooks/permission`, `/hooks/event` 204 + mpsc); Policy/Store/Notice tạm cho T8/T6/T5. Q1b đã trả lời (2.1.308, lái ConPTY): Claude Code **không** đóng kết nối khi trả lời ở terminal → hub hủy khi tới `PostToolUse` cùng tool/input hoặc `Stop`/`UserPromptSubmit`/`SubagentStop` của cùng agent. 31 test tích hợp + 3 đơn vị, workspace xanh, clippy/fmt sạch. Security audit: không có mức cao; đã sửa finding 1–3, 5; finding 4 chấp nhận (ghi ở PLAN T3). Budget: idle 8,7 MB WS / 1,5 MB private, 20 request giữ 9,9 MB, CPU idle 0%, p99 ingest 1,46 ms. · Vướng: không có YouTrack MCP trong phiên (người cập nhật ARC-24); đề xuất ticket mới dưới story T5: token riêng cho UI/WebSocket, không nằm trong môi trường Claude Code (audit follow-up 1); CLAUDE.md còn ghi Q1b "CHƯA kiểm chứng", chờ duyệt sửa.
