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
| T0 · ARC-21 | main | Scaffold: Cargo workspace (`hub-core`, `hub`, `replay`), Tauri 2 trong `crates/app`, `ui/` Vite+React+TS, CI chạy fmt/clippy/test | — | 2 | done |
| T1 · ARC-22 | main | Hook lab: hub "ghi băng" tối giản nhận mọi hook và lưu JSON thô; ghi fixture từ 3–4 phiên thật; trả lời Q1–Q4 trong HOOK_FINDINGS.md | T0 | 2 | done |
| T2 · ARC-23 | main | Chốt `schema.rs` (Event v1) + chuẩn hóa từ JSON hook, test với toàn bộ fixture | T1 | 1 | done |

**Tiêu chí chấp nhận**
- T0: `cargo test --workspace`, `pnpm -C ui test`, `pnpm -C crates/app tauri dev` đều chạy được; CI xanh.
- T1: ≥30 fixture bao phủ SessionStart, PreToolUse (Edit/Read/Bash), PermissionRequest, Stop, SubagentStart/Stop; Q1–Q4 có kết quả kèm bằng chứng.
- T2: mọi fixture parse thành Event không lỗi; có test snapshot.

## Giai đoạn B — các làn song song (tuần 2–5)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T3 · ARC-24 | hub | Luồng duyệt: `/hooks/permission` giữ request bằng oneshot, timeout 110 s trả `{}`, hủy khi SessionEnd | T2 | 3 | done |
| T4 · ARC-25 | hub | State machine agent theo bảng trong ARCHITECTURE.md; persona từ `arcturus.toml` | T2 | 2 | done |
| T5 · ARC-26 | hub | WebSocket `/ws`: snapshot + delta; viết `docs/PROTOCOL.md`; xác thực token; xử lý `TeamPause`/`TeamResume` phía hub; `GET /approvals/{id}/input` | T3, T4 | 2 | done |
| T5c · ARC-43 | hub | Trần byte cho `tool_input` đang giữ (`held_input_max_bytes` 8 MiB), đặt chỗ trước khi đọc body, giới hạn độ dài trường | T5 | 1 | done |
| T5d · ARC-44 | hub | F6: `--ui-token-stdout`, sàn thời gian duyệt `min_review_ms`, luật danger cho T8, ghi giới hạn bảo mật | T5c | 1 | done |
| T6 · ARC-27 | hub | SQLite WAL: events (TTL 30 ngày), approvals; ring buffer trong RAM | T2 | 2 | done |
| T7 · ARC-28 | hub | Đối soát `claude agents --json` mỗi 30 s, dọn phiên chết | T4 | 1 | done |
| T8 · ARC-29 | policy | Danger list + chuẩn hóa lệnh (bỏ ký tự ẩn/bidi), tính `risk` | T2 | 2 | done |
| T9 · ARC-30 | policy | Batch rule ("cho phép mẫu X trong N phút") + gom yêu cầu giống nhau | T8 | 2 | done |
| T10 · ARC-31 | policy | Trust grant cơ bản: đếm chuỗi duyệt liên tiếp, đề xuất nâng mức, tự hạ khi bị deny/hoàn tác | T8, T6 | 3 | done |
| T11 · ARC-32 | ui | Component nhân vật SVG (mood × palette), port từ mockup | T0 | 1 | done |
| T12 · ARC-33 | ui | Tray + badge số người giơ tay; popup Lite (hàng đợi, duyệt/từ chối, duyệt gộp) | T5, T11 | 3 | done |
| T13 · ARC-34 | ui | Văn phòng 2D: lưới bàn, trạng thái, "phòng hỗ trợ" gom agent rảnh (đội hình chính ≤5) | T5, T11 | 3 | review |
| T14 · ARC-35 | ui | Nút "Tạm dừng cả đội" (chỉ UI, hub ở T5), "Giờ tập trung"; đếm ngược phủ quyết cho yêu cầu đảo ngược được | T12 | 2 | done |
| T15 · ARC-36 | lab | `tools/replay`: phát lại fixture với số phiên/tốc độ tùy chọn | T2 | 1 | done |
| T16 · ARC-37 | lab | Kịch bản đo ngân sách + bảng lịch sử trong BUDGET.md; chạy trên máy yếu | T15, T5 | 1 | review |
| T17 · ARC-38 | lab | Test e2e: Claude Code thật (headless `-p`) + hub, kiểm tra allow/deny/timeout | T3 | 2 | done |

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

**Ghi chú từ ARC-43/44** (audit `security-auditor` 2026-10-22)
- T8: thêm luật danger `arcturus_ui_token` (PLAN T8, B2). File cấu hình nên chuyển khỏi thư mục làm việc trước T8/T10
  (agent ghi được workspace thì sửa được `min_review_ms`, `held_input_max_bytes`).
- T9: duyệt gộp/batch phải gọi `approvals::allow_gate` cho từng yêu cầu (sàn thời gian, đếm ngược, thẻ đã cắt).
- T10: đếm ngược đi qua sàn `min_review_ms`; kiểm khi đọc cấu hình rằng đếm ngược ≥ sàn.
- T12: app khởi động hub với `--ui-token-stdout`, đọc token từ dòng đầu stdout và **luôn đọc stderr** (pipe đầy thì hub
  bị chặn); UI làm mờ nút Allow trong `min_review_ms` từ lúc hiện thẻ.

**Ghi chú từ T6/T8** (chi tiết ở `docs/PLAN.md` "Đã làm ở T6", "Đã làm ở T8")
- T9/T10: cắm vào `RulesPolicy::with(Box<dyn Allowance>)`; mọi kết quả đi qua `policy::enforce` (Critical và High không bao
  giờ tự duyệt, đếm ngược chỉ khi `reversible`). `ApprovalCtx` chưa có `subject` (tên persona): T10 thêm. Migration v2
  thêm vào `MIGRATIONS` trong `store.rs`. Đọc DB bằng `StoreReader` (kết nối riêng), ghi chỉ qua `StoreHandle`.
- T12: thẻ duyệt có `danger_spans` thật (chuỗi mẫu `echo "🚀 thư mục" && rm -rf đầu-ra` → `[[20,33]]`), `suspicions`
  đủ 4 loại, `risk`, `reversible`.
- T19: `Hub::recent_events(n)` (ring 2.000 sự kiện) và bảng `events`/`approvals`.
- Hub mặc định đọc `<config_dir>/arcturus/arcturus.toml` và ghi `<data_dir>/arcturus/arcturus.db`; mọi lệnh chạm hai file
  này hoặc token UI là Critical (`arcturus_state`, `arcturus_ui_token`).

## Giai đoạn C — dogfood và mốc v0 (tuần 6)

| ID · YouTrack | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| T18 · ARC-39 | lab | `hooks/arcturus-dogfood.json` + hướng dẫn cài/gỡ; bật dogfood | T5, T12 | 1 | review |
| T19 · ARC-40 | hub | Standup sinh từ template (sự kiện + git log), 0 token | T6 | 2 | done |
| T20 · ARC-41 | lab | Đo chỉ số: thời gian xử lý trung bình mỗi yêu cầu, thời gian agent tự chạy; so trước/sau | T18 | 1 | review |
| T21 · ARC-42 | main | `/milestone-gate v0` | tất cả | 1 | review |

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
- 2026-10-22 · hub · T4/ARC-25 state machine và persona (nhánh `arc-25-state-persona`, commit `b8e8563`, rebase lên main sau khi PR T3 merge): `hub-core/src/state.rs` (`TeamState::apply/tick/next_wakeup` theo §3.6, tạo agent từ sự kiện đầu tiên, chỉ gửi khi thay đổi nhìn thấy được), `persona.rs` (globset, Windows, thư mục cha, tên sao nhả lại khi xóa, FNV-1a), `config.rs` (`arcturus.toml`, lỗi kèm số dòng, khóa lạ là lỗi, timeout 1–115 s, bind loopback, màu `#RRGGBB`), `protocol.rs` (phần T4: `AgentView`, `Palette`, `AgentUpdated`/`AgentRemoved`). §3.1 thêm `Event.cwd`, `AgentRef.agent_type` (Option, không tăng `v`, đã duyệt); 11 snapshot cập nhật. 56 test đơn vị + 1 test phát lại 64 fixture, workspace xanh, clippy/fmt sạch; thử cố ý làm hỏng 2 chỗ thì test đỏ. Không đo ngân sách: binary hub chưa dùng state (nối vào hub sau khi PR T3 merge). · Vướng: chưa nối vào `crates/hub` (vòng sự kiện, bước 3 `/hooks/permission`, `Config` thay `--approval-timeout`, `tick` theo `next_wakeup`, đồng bộ `paused`).
- 2026-10-22 · hub · T5/ARC-26 (T5a PR #3 đã merge; T5b nhánh `arc-26b-websocket-ws`): lệnh quá dài trả về terminal (bất biến 3 mới), `/ws` + token UI riêng (F6), `/approvals/{id}/input`, `docs/PROTOCOL.md`; audit T5b không có mức cao, đã sửa F2, F3, F5–F8. Budget 78fbde3: idle 11,1 MB, 20 phiên + 2 client WS 13,3 MB, CPU idle 0%, p99 1,49 ms. · Vướng: 64 Write ~1 MB giữ cùng lúc lên 93 MB (có trần, vượt 40 MB) — cần trần byte cho tool_input đang giữ; F1 (token UI đọc được bởi agent có quyền chạy mã) chờ người quyết; 1964793 chưa đo lại.
- 2026-10-22 · main · Đóng T0–T5 (PR #1–#4 đã merge): bảng task chuyển `done`; CLAUDE.md ghi kết quả Q1b đã kiểm chứng ở T3; xóa 9 nhánh local đã merge và 4 worktree `arc-24…26b`. · Vướng: hai điểm treo từ T5b (trần byte `tool_input` đang giữ: 93 MB > 40 MB; F6 token đọc được bởi agent) chưa có ticket.
- 2026-10-23 · policy · T8/ARC-29 (nhánh `claude/dreamy-davinci-oue6io`, commit `2bd289e`): `hub-core/src/policy/` (normalize, danger, risk, `Policy`/`assess`/`enforce`/`RulesPolicy`/`Allowance`); 9 luật PLAN + mở rộng + `arcturus_ui_token` (B2) + `arcturus_state` + `unanalyzed`; hub dùng đánh giá thật cho thẻ (`danger_spans`, `suspicions`, `risk`, `reversible`) và lọc mọi policy qua `enforce`; `--config` mặc định ở thư mục cấu hình người dùng. 41 test policy + test tích hợp hub. · Vướng: chưa có security audit riêng cho T8; các điểm lệch cần duyệt ở PLAN "Đã làm ở T8" (WebFetch ra ngoài là Medium; `python -c` chỉ High).
- 2026-10-23 · hub · T6/ARC-27 (cùng nhánh, commit `699e221`): `hub-core/src/store.rs` (SQLite WAL, một thread ghi, ack sau commit, bỏ events khi đầy, TTL 6 giờ, `StoreReader`, `EventRing`), hub ghi mọi sự kiện và quyết định, `--db`. 11 test store + 5 test tích hợp. Budget (container Linux, so với main cùng máy): idle 8,9 MB (main 6,0), 20 phiên 14,4 MB (main 7,1), CPU idle 0%, p99 ingest 2,71 ms; stress 64 × 1 MB đỉnh 35,6 MB (main 27,3). · Vướng: `ws_disconnects_stalled_client` (ARC-45) vẫn chập chờn trên Linux, cả trên main 3cf195d (1/5 lượt): client đọc bị tụt kênh broadcast nên nhận dưới 8 MB; đề xuất ticket mới dưới story T5. Chưa đo trên máy chuẩn Windows.
- 2026-10-23 · ui · T11/ARC-32 (nhánh `claude/dreamy-davinci-oue6io`): `ui/src/components/Character.tsx` theo phụ lục A (5 mood render có điều kiện, không dùng opacity), `role="img"` + `aria-label` theo mood, màu chỉ nhận `#RRGGBB`, hiệu ứng 450 ms một lần khi chuyển sang approval/done, tắt khi `prefers-reduced-motion`. 17 test mới (tổng UI 18), typecheck và `vite build` sạch; đã soát hình 5 mood × 3 bảng màu bằng ảnh chụp Chromium. · Vướng: không.
- 2026-10-23 · ui · T12/ARC-33 (cùng nhánh): app Tauri tray + popup Lite (`crates/app/src-tauri/src/{main,tray,hub_proc,link,http}.rs`), UI `protocol.ts`/`store.ts`/`ws.ts`/`hub.ts`, `ApprovalCard`, `HandoffCard`, `QueueList`, `views/LitePopup`; hub `--exit-on-stdin-close`. Sửa lỗi hub: stderr gãy làm tracing-subscriber panic, giết future tắt hub (`log_internal_errors(false)`, test hồi quy). UI 52 test, app 10 test, hub thêm 2 test; E2E với hub thật: Enter duyệt / Esc từ chối ra đúng JSON cho hook, thẻ cắt chặn Enter; app bị kill thì hub tắt êm và trả `{}`. · Vướng: chưa đo được popup ≤300 ms và RSS app trên máy chuẩn (container không có tray thật, không GPU: app tray 100 MB RSS, heap 20 MB); UI dùng hằng `min_review_ms` 1.000 ms, đề xuất ticket thêm giá trị này vào `TeamView`.
- 2026-10-23 · main · Đóng T5c, T5d, T6, T8, T11, T12 (PR #6–#10 đã merge): bảng task chuyển `done`. · Vướng: ARC-27/29/32/33/43/44 trên YouTrack chờ người chuyển `Done`; T12 còn phải đo popup ≤300 ms và RSS app trên máy chuẩn Windows.
- 2026-10-26 · lab · T18/ARC-39 (nhánh `claude/dreamy-davinci-oue6io`): `hooks/arcturus-dogfood.json` (SessionStart đổi sang hook command + curl, F1), `scripts/dogfood-install.sh`/`dogfood-uninstall.sh`/`dogfood-lib.sh` (gộp/gỡ bằng jq, có sao lưu, `--dry-run`, `--port`, `--settings`, giữ hook người dùng), `scripts/test-dogfood.sh` (54 kiểm tra, kiểm đột biến bắt 10/11, đột biến còn lại tương đương), job CI `scripts`, `docs/DOGFOOD.md`. Đã chạy lệnh SessionStart đã cài với hub thật: đúng token vào DB; hub tắt, sai, thiếu token đều im lặng. · Vướng: chưa thử với Claude Code thật (container không đăng nhập): các hook http khi hub lỗi/hết giờ vẫn là mục T17. Người dùng cần tự chạy `dogfood-install.sh` trên máy mình và làm các phép thử ở `docs/DOGFOOD.md` mục 5–6.
- 2026-10-26 · ui · T13/ARC-34: Office + AgentDesk, sắp mood/thời gian, phòng hỗ trợ chỉ gom idle vượt roster_max (không giấu agent đang hoạt động hoặc agent con); chọn agent cuộn/focus thẻ; dùng chung kết nối và điều kiện duyệt với popup. Test UI/build và Chromium với hub thật: Allow/Deny, handoff, 7 agent/roster 3, mobile 390px, xóa agent. Profiler sau mô phỏng Page Visibility hidden 1,2 s: 0 Paint/FireAnimationFrame/TimerFire. · Vướng: chưa đo CPU/RSS và vòng đời webview/ẩn cửa sổ trên Tauri Windows thật; chi tiết docs/OFFICE.md.
- 2026-10-26 · ui · T14/ARC-35 (nhánh `arc-35-pause-focus`): `TeamControls` (tạm dừng/tiếp tục, giờ tập trung) ở popup và Office, đếm ngược phủ quyết (`countdown.ts`, `useApprovalQueue`: chỉ Low/Medium + reversible + không tạm dừng, gửi `origin: countdown` đúng một lần, "Dừng lại"/"Chạy ngay" gửi `origin: user`), reducer xóa `countdown_ms` khi tạm dừng; tray: chấm báo/tooltip theo giờ tập trung và mục menu. UI +13 test, app +1 test. · Vướng: không (giờ tập trung chỉ ẩn chấm báo/tooltip và đồng hồ đếm ngược tính từ lúc thẻ hiện đã được chốt vào đặc tả PLAN §T14); hub chưa gửi `countdown_ms` thật tới khi T10 xong nên đếm ngược mới được kiểm bằng test UI; chưa thử trên app Tauri thật.
- 2026-10-27 · lab · T17/ARC-38 (nhánh `arc-38-e2e-claude`): `crates/hub/tests/e2e_claude.rs`, 4 kịch bản `#[ignore]` với Claude Code 2.1.308 thật (allow, deny, hết giờ, hub tắt): 4/4 đạt trong 39 giây; xác nhận trên máy thật hết giờ và lỗi kết nối không bao giờ allow. · Vướng: chưa kiểm phản hồi non-2xx; không chạy trong CI mặc định; còn lại một câu "CHƯA kiểm chứng" trong CLAUDE.md cần người cập nhật.
- 2026-10-27 · policy · T9/ARC-30 (nhánh `arc-30-batch-group`): `policy/group.rs` (`group_key`, lệnh ghép có hậu tố), `policy/batch.rs` (`BatchRules` + `Allowance`), `Approvals::decide_group` qua `allow_gate`, `Hub::batch_open/close/restore_batch`, migration v2 `batch_rules`, WebSocket `decide_group`/`batch_open`/`batch_close`, UI "Duyệt cả N" gửi một `decide_group`; 18 test đơn vị + 13 tích hợp + 2 UI. · Vướng: chưa có UI mở/đóng batch và Snapshot chưa liệt kê quy tắc đang mở (đổi hợp đồng §3.4); migration v3 sẽ do T10 thêm (lệch nhẹ với PLAN "v2 (T9, T10)").
- 2026-10-27 · policy · T10/ARC-31 (nhánh `arc-31-trust-grant`, chồng lên `arc-30-batch-group`): `policy/trust.rs` (`TrustBook` + `Allowance`: mức 6 tự duyệt, mức 5 đếm ngược 10 giây, chỉ tới Medium), `ApprovalCtx.subject`, `ServerMsg::Suggestion`, migration v3 (`trust_grants`, `streaks`, `trust_log`), hub nạp lại khi khởi động, `SuggestionAnswer`, UI `SuggestionList`; 14 test đơn vị + 9 tích hợp + 5 UI. · Vướng: PR phải merge sau T9; chưa có UI thu hồi grant; hoàn tác tự động chờ đo được; `declined_until` chỉ trong bộ nhớ.
- 2026-10-27 · hub · T7/ARC-28 (nhánh `arc-28-reconcile`): `hub/src/reconcile.rs` (`claude agents --json`, parse chịu lỗi, dọn phiên vắng hai lần quan sát thành công liên tiếp và đã im lặng >= 2 chu kỳ, bằng `SessionEnd` giả qua `Hub::ingest`), `TeamState::sessions()`, trả lời một phần Q8; không chạy `claude` khi hub không có phiên; 7 test đơn vị + 6 tích hợp + 1 test `#[ignore]` với `claude` thật (270 ms). · Vướng: chưa biết phiên `-p` và `--bg` có được liệt kê không (Q8, kiểm ở T17); chưa đo ngân sách hub với task đối soát.
- 2026-10-27 · hub · T19/ARC-40 (nhánh `arc-40-standup`): `hub-core/src/standup.rs` (đếm theo persona, template tiếng Việt, làm sạch chuỗi không tin cậy), `StoreReader::events_since`, `hub/src/standup.rs` (`SystemGit` có timeout), `GET /standup?hours=` (token UI, `no-store`), `Hub::set_db_path`; 8 test đơn vị + 6 tích hợp (SQLite và `git log` thật). · Vướng: chưa có UI hiển thị (v1); "task xong" đếm theo lượt trả lời, chưa theo ticket.
- 2026-10-27 · lab · T15/ARC-36 (nhánh `arc-36-replay`): `tools/replay` gửi fixture tới hub (số phiên, tốc độ, thời lượng, `--permission-mode allow|skip`, người duyệt giả qua WebSocket, p50/p95/p99, `--assert-p99-ms`), tách thư viện + binary; 9 test đơn vị + 6 test tích hợp với hub thật. Hub release, 5 phiên, 200/giây: p99 0,93 ms. · Vướng: không.
- 2026-10-27 · main · Đồng bộ bảng với YouTrack sau khi merge PR #16–#22: T7, T9, T10, T15, T17, T19 và T14 → `done`. T6 (ARC-27) và T12 (ARC-33) trở về `review` vì tiêu chí ngân sách còn chưa đo đủ trên Windows (hub idle 18,2 MB WS so với 15 MB; popup bản release chưa đo, bản dev 300–380 ms so với ≤300 ms); T13 (ARC-34) và T18 (ARC-39) giữ `review` (profiler tab ẩn, cài/gỡ trên máy sạch). Còn `todo`: T16, T20, T21. Chi tiết ở comment từng ticket. · Vướng: không.
- 2026-10-27 · lab · T16/ARC-37 (nhánh `arc-37-measure`): `scripts/measure.ps1`, `scripts/measure.sh`, chế độ đo popup trong app (`ARCTURUS_MEASURE_POPUP`), test của script. Đo lại trên Windows: hub ĐẠT (idle 13,4 MB, đỉnh 16,7 MB, p99 0,32 ms) nên T6/ARC-27 → `done`. T12/ARC-33 → `done` sau khi thêm prewarm khi rê chuột: popup mở 44 ms kể từ lúc bấm khi con trỏ đã vào icon 500 ms trước (đường nguội ≈400 ms), sau đóng 32,8 MB; popup mở vẫn 367 MB WS so với ≤150 MB. Chi tiết ở `docs/BUDGET.md`. · Người dùng đã tự kiểm bằng chuột thật trên bản release (2026-10-27): thấy `popup_prewarmed` trước `popup_shown` và `popup_ready` dưới 300 ms. · Vướng: không.
- 2026-10-27 · lab · T20/ARC-41 (nhánh `arc-41-metrics`): `scripts/metrics.sql` (5 truy vấn), `scripts/metrics.py` (không cần sqlite3 CLI, chỉ đọc), test `hub-core/tests/metrics_sql.rs` (7, kiểm đột biến), `docs/DOGFOOD.md` mục 10. Số liệu thật chưa có: baseline phải đo tay và dogfood ≥ 2 tuần chưa diễn ra (DB trên máy dev chỉ có 8 yêu cầu thử). · Vướng: cần người dùng đo baseline và dùng hằng ngày.
- 2026-10-27 · main · T21/ARC-42 chạy `/milestone-gate v0` (nhánh `arc-42-gate-v0`, không code mới): **CHƯA ĐẠT**. Báo cáo ở `.squad/reports/2026-10-27-milestone-v0.md`. Chặn bởi 3 phát hiện bảo mật mức Cao ở vùng khóa (brace expansion/tùy chọn viết tắt/tham số PowerShell, tách bằng dấu nháy và `127.1`, `rm -r`/`sed -i` ngoài workspace chỉ Medium; đã tái hiện bằng `policy::assess`), 4 task chưa chốt, popup mở chưa vào ngân sách, chưa có số dogfood. · Vướng: cần người quyết định về `git push`/`rm -r` (CLAUDE.md quy tắc 2 so với PLAN.md:890) và về ngưỡng popup.
- 2026-10-27 · policy · ARC-29 sửa sau cổng v0 (nhánh `arc-29-danger-evasions`): đóng 3 phát hiện mức Cao (brace expansion → unparsed/High, tùy chọn dài viết tắt theo tiền tố, `-Recurse:$true`, needles/URL hub trên từ đã bỏ dấu nháy, `127.1`/hex/thập phân/`[::ffff:…]`, đích ghi `touch/mkdir/sed -i/chmod/Set-Content…`); 6 test hồi quy; `cargo test --workspace` và clippy xanh. Chưa đụng `rm -r`/`git push` thường (chờ người quyết định). · Vướng: không.
- 2026-10-27 · policy · ARC-29 theo quyết định của người dùng (CLAUDE.md quy tắc 2): `rm -r` ngoài workspace, `git push/fetch/pull/clone/ls-remote` tới URL lạ và lệnh mạng ra ngoài (`gh`, `npm/cargo publish`, `docker push`, `nslookup/dig/ping/…`, `Start-BitsTransfer`) là vùng khóa; 3 test hồi quy mới + 12 ca trong `critical_samples`; `PLAN.md` đồng bộ; `cargo test --workspace` và clippy xanh. · Vướng: không.
- 2026-10-27 · lab · ARC-33/ARC-37 (nhánh `arc-33-popup-budget`): theo quyết định của người dùng, RAM popup mở tính theo private working set (74,8–75,4 MB, ≤150 MB đạt; working set 362–364 MB đếm trùng trang dùng chung), thêm ngưỡng đường nguội ≤500 ms (403 ms trung vị); `measure.ps1 -Target app` cho `verdict=PASS` ở cả `-HoverMs 500` (58 ms) và `-HoverMs 0` (403 ms). · Vướng: đường nguội vẫn ≈400 ms (> 300 ms của đường chuột).
- 2026-10-28 · main · ARC-42 chạy lại kiểm tra cổng v0 trên máy thật (nhánh `arc-42-gate-v0-fixes`): `cargo test --workspace` 418 ĐẠT, clippy (cả `crates/app`), fmt, `pnpm -C ui test` 101 và build ĐẠT; e2e Claude Code 2.1.308 thật 5/5 ĐẠT (allow, deny, hết giờ, hub tắt, non-2xx 500/401: không bao giờ allow, không treo); `measure.ps1` hub và app ĐẠT (`docs/BUDGET.md`). Đã sửa phát hiện 5–7: `git log` của standup tắt kiểm chữ ký và bỏ `cwd` UNC, duyệt gộp chỉ cộng một lần vào chuỗi trust, route hook kiểm `Host` và hàng đợi `/hooks/event` có ngân sách 8 MiB; 4 test hồi quy (hai cái đã thử đột biến), docs `PROTOCOL.md`/`CLAUDE.md`/`HOOK_FINDINGS.md`, thêm `pnpm audit` vào CI. · Còn lại (cần người hoặc cho phép cài): dogfood ≥ 2 tuần và baseline (T20), T13 trên cửa sổ Tauri thu nhỏ thật, T18 cài/gỡ trên máy sạch (thiếu `jq` trên máy này), `cargo audit` (chưa cài). · Vướng: không.

## Giai đoạn v1 — Trợ lý 3D "Pet" (đề xuất, chưa có mã ARC, chưa duyệt)

Chi tiết, bất biến riêng và thiết kế: `docs/PLAN.md` §7. Thuộc Epic ARC-6. Chỉ đề xuất ticket, chưa tạo. Đối chiếu với `main`
808c436 ngày 2026-10-28 (§7.9).

| ID | Làn | Task | Phụ thuộc | Ước lượng | Trạng thái |
|---|---|---|---|---|---|
| P0 | lab + ui | Spike (nhánh bỏ đi, không merge): cửa sổ trong suốt + VRM + VRMA, thử click xuyên và chi phí theo dõi con trỏ, CSP cho three-vrm, đo RAM/GPU/FPS bằng `scripts/measure.ps1` trên máy này và một máy yếu, chốt ngân sách | T21 (cổng v0 đạt) | 2 | todo |
| P1 | main + ui | ADR + sửa phạm vi CLAUDE.md/ARCHITECTURE; đưa điểm thay thế UI từ nhánh `arc-ui-extensibility` lên main (đang xung đột 3 file); thêm `AvatarStyle = 'vrm'` | P0 | 2 | todo |
| P2 | ui | Cửa sổ `pet` tạo ở Rust như `popup`/`office` (`tray.rs`), capability riêng, tray bật/tắt, kéo, nhớ vị trí, đa màn hình/DPI, hủy khi đóng | P1 | 3 | todo |
| P3 | ui | `?view=pet` + `PetScene` nạp lazy (không vào bundle của popup), nền trong suốt, vẽ theo `useMotionPolicy`, lui về 2D | P2 | 3 | todo |
| P4 | ui | Trạng thái → biểu cảm/động tác (VRMA), chớp mắt, nhìn theo con trỏ, ngủ | P3 | 3 | todo |
| P5 | ui | `brain.ts` tầng 0 + bong bóng thoại (duyệt qua `useApprovalQueue`/`allowGate`, Critical không có nút) | P3 | 3 | todo |
| P6 | ui | Tương tác: bấm, bấm đúp mở Văn phòng, kéo, vuốt ve, menu chuột phải, bàn phím/tray | P4, P5, T13 | 2 | todo |
| P7 | ui | Orchestrator tầng 1: lệnh ánh xạ `team_pause`/`team_resume`/`focus_set` và mở thẻ; "tóm tắt" qua lệnh Rust mới gọi `GET /standup`; không bao giờ gửi `decide*`, `batch_open`, `suggestion_answer` | P5 | 2 | todo |
| P8 | ui + lab | Tài sản: model gốc, giấy phép, trang phục, nạp model người dùng qua lệnh Rust có kiểm tra | P3 | 3 | todo |
| P9 | ui | Giọng nói + lip sync (tùy chọn) | P7 | 4 | todo |
| P10 | main | Nghiên cứu orchestrator tầng 2 (LLM, giao việc có xác nhận): ADR bảo mật | P7 | 2 | todo |
| P11 | lab | `measure.ps1 -Target pet` + chế độ đo pet trong `measure.rs`, đo ngân sách và dogfood pet | P6 | 2 | todo |
