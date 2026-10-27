# Kiến trúc Arcturus (v0 và hướng mở rộng)

## Thành phần

```
┌──────────────── Máy người dùng ─────────────────────────────────────────────┐
│                                                                              │
│  Claude Code (N phiên, mỗi phiên 1 worktree)                                 │
│     │ HTTP hooks: SessionStart, PreToolUse, PostToolUse, PermissionRequest,   │
│     │             Stop, SubagentStart/Stop, Notification, SessionEnd          │
│     ▼                                                                        │
│  arcturus hub (Rust, 127.0.0.1:7878)                                          │
│   ├─ ingest      POST /hooks/event       (trả 204 ngay, ≤5 ms)               │
│   │              POST /hooks/permission  (giữ request tới khi có quyết định) │
│   ├─ normalize   JSON hook → Event (schema chung)                            │
│   ├─ state       agents · sessions · approvals · tasks (trong RAM)           │
│   ├─ policy      danger list → trust grants → batch rules → hỏi người        │
│   ├─ store       SQLite WAL: events (TTL 30 ngày), approvals, trust_grants   │
│   ├─ reconcile   mỗi 30 s: `claude agents --json` để dọn phiên chết          │
│   └─ pubsub      GET /ws (WebSocket): snapshot + delta                       │
│                         │                                                    │
│        ┌────────────────┼──────────────────┐                                 │
│        ▼                ▼                  ▼                                 │
│   Tray + popup Lite   Văn phòng 2D      (v1) VRM view · (v3) ESP32 · PWA     │
│   (Tauri, mặc định)   (mở khi cần)                                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

Hub là daemon riêng; app chỉ là client. Đóng popup hay văn phòng thì app vẫn sống trong tray, hub vẫn chạy và giữ hàng
đợi. Nếu chưa có hub, app khởi động hub làm tiến trình con (`--ui-token-stdout --exit-on-stdin-close`, T12): token UI chỉ
app biết, nên khi app thoát (menu "Thoát") hay crash, hub con tắt êm và trả `{}` cho mọi yêu cầu đang chờ (Claude Code hỏi
trong terminal). Hub người dùng tự khởi động thì app chỉ kết nối (đọc file token), thoát app không tắt hub đó.

## Luồng duyệt (PermissionRequest)

1. Claude Code gửi POST `/hooks/permission` với JSON đầu vào của hook.
2. Hub chuẩn hóa thành `Event{type: approval.requested}`, tính `risk` bằng policy (không tin agent tự khai).
3. Policy quyết theo thứ tự (chi tiết: `docs/PLAN.md` T3, T8–T10, T14):
   1. Khớp **danger list** (risk Critical) → luôn hỏi người (không bao giờ tự allow).
   2. Có **trust grant** còn hạn cho (persona) × (`group_key`) ở mức 6 và risk ≤ Medium → tự allow, ghi log
      `decided_by=policy`. Mức 5 → vẫn hỏi người; đếm ngược phủ quyết chỉ hiện khi yêu cầu `reversible`
      (v0: chỉ risk Low).
   3. Có **batch rule** đang mở cho `group_key` ("cho phép `Bash:npm test` 1 giờ") và risk ≤ Medium → tự allow.
   4. Còn lại → vào hàng đợi.

   Đội đang **tạm dừng** (`team.paused`): mọi kết quả tự allow ở bước 2–3 bị đổi thành hỏi người, và **không có đếm
   ngược** (bấm tạm dừng thì hủy luôn đếm ngược của yêu cầu đang chờ; tiếp tục không khôi phục). Hub chặn
   `decide` có `origin: countdown` khi tạm dừng, nên khi tạm dừng không có allow nào mà người không bấm. Tạm dừng
   không tự từ chối và ở v0 không dừng được agent thật (xem T14).
   "Giờ tập trung" (`team.focus`) chỉ là cờ của UI/tray: ẩn chấm báo và thông báo của yêu cầu Low/Medium, hub không đổi
   hành vi duyệt và thẻ vẫn nằm trong hàng đợi.
   Tự allow chỉ được trả sau khi bản ghi `approvals` đã ghi xong; ghi lỗi thì đổi thành hỏi người.
4. Vào hàng đợi: tạo `oneshot` channel, broadcast thông điệp WebSocket `approval_requested`.
5. Người quyết qua UI → hub gửi vào channel → trả JSON decision cho Claude Code.
6. Hết 110 giây (timeout hook = 120) → trả `{}`. Claude Code tự hỏi trong terminal. Đánh dấu `decided_by=timeout`.
7. Mọi quyết định ghi vào bảng `approvals` (ai, lúc nào, qua thiết bị nào, độ trễ).

Trước bước 3: lệnh quá dài để duyệt trên thẻ (xem mục WebSocket) không vào policy, không vào hàng đợi; hub trả `{}`
ngay, ghi `handed_off`.

Sơ đồ trạng thái một yêu cầu: `pending → allowed | denied | timed_out | cancelled(session_ended)`; hoặc `handed_off`.

## Suy ra trạng thái agent (0 token)

Bảng đầy đủ theo tool (kể cả `act_text`): `docs/PLAN.md` §3.2 và §3.6.

| Sự kiện | activity | mood nhân vật |
|---|---|---|
| SessionStart | idle | idle |
| UserPromptSubmit | thinking | working |
| PreToolUse Edit/Write/MultiEdit | typing | working |
| PreToolUse Read/Grep/Glob | reading | working |
| PreToolUse Bash | running | working |
| PostToolUse | thinking | working |
| PermissionRequest | waiting | approval |
| PostToolUseFailure, StopFailure | — (giữ nguyên) | error (giữ tới sự kiện kế tiếp) |
| Stop | idle | done (3 giây) → idle |
| SubagentStart/Stop | tạo/xóa agent con `kind=subagent` | |
| SessionEnd | xóa agent | |

## Định danh và persona

- Khóa agent: `session_id` → `agt_<session_id>`. Agent con: `session_id + agent_id` từ SubagentStart →
  `agt_<session_id>_<agent_id>`.
- Persona gán qua `arcturus.toml` theo `cwd` (glob `match_cwd`, `docs/PLAN.md` §3.7). Đường dẫn Windows được đổi
  `\` thành `/` trước khi so; thư mục con của worktree cũng khớp:

```toml
[[persona]]
match_cwd = "**/.claude/worktrees/ui*"
name = "Linh"
role = "frontend"
palette = { hair = "#2E2A5C", outfit = "#8B7CF6", acc = "#FF7A59" }
```

Không khớp: tự đặt tên từ danh sách mặc định, màu theo hash của session_id.

## Schema sự kiện chung (v1)

Nguồn sự thật: `crates/hub-core/src/schema.rs`, đặc tả ở `docs/PLAN.md` §3.1. Ví dụ dưới đây chỉ minh họa;
trường nào có nhiều giá trị thì liệt kê các giá trị hợp lệ, phân cách bằng `|`.

```json
{
  "v": 1,
  "id": "evt_01J...",
  "ts": "2026-10-21T09:15:02.123Z",
  "host": "laptop",
  "source": "claude-code",
  "session_id": "abc123",
  "cwd": "D:\\code\\Arcturus",
  "agent": { "id": "agt_abc123", "parent_id": null, "kind": "session | subagent | teammate", "agent_type": "general-purpose" },
  "type": "session.started | prompt.submitted | tool.pre | tool.post | tool.failed | approval.requested | approval.resolved | subagent.started | subagent.stopped | notification | turn.stopped | turn.failed | session.ended | unknown",
  "activity": "typing | reading | running | thinking | waiting | idle",
  "tool": { "name": "Bash", "summary": "npm test", "risk": "low | medium | high | critical" },
  "approval": { "request_id": "req_01J...", "deadline_ms": 110000, "decided_by": "user | countdown | policy | timeout | cancelled", "decision": "allow | deny" },
  "metrics": { "context_pct": 0.62 }
}
```

- `activity`, `tool`, `approval`, `metrics` có thể là `null`; `tool.risk` chỉ policy được điền.
- `cwd`, `agent.agent_type` (thêm ở T4, không tăng `v`): có thể là `null`; JSON cũ thiếu hai trường này vẫn đọc được.
  `agent_type` chỉ có ở subagent.
- `type: "unknown"`: sự kiện hook lạ được giữ lại, không bao giờ làm hub trả lỗi.
- Tên, vai trò, bảng màu không nằm trong Event: hub gắn persona khi dựng `AgentView` (`docs/PLAN.md` §3.4).
- JSON hook gốc lưu ở cột `events.raw_json`, tra theo `id` của Event (Event không có trường trỏ riêng).
- UI chỉ nhận `AgentView`/`ApprovalView`, không nhận Event thô. `AgentView` chỉ có tóm tắt hoạt động.
  `ApprovalView` mang lệnh cần duyệt: Bash và đường dẫn không bao giờ bị cắt; nội dung Edit/Write bị cắt còn
  2.000 ký tự mỗi trường (`truncated = true`), bản đầy đủ lấy qua `GET /approvals/{request_id}/input`
  (`docs/PLAN.md` §3.4).

## Bảng SQLite

DDL chính xác: `docs/PLAN.md` §3.5. Migration đánh số bằng `PRAGMA user_version`. File `<data_dir>/arcturus/arcturus.db`
(`hub --db`), WAL. Một thread ghi duy nhất (`hub_core::store`), thứ tự ghi đúng thứ tự gửi; sự kiện có thể bị bỏ khi
kênh đầy, bản ghi duyệt thì không; AutoAllow chỉ trả `allow` sau khi bản ghi đã commit. Chi tiết: `docs/PLAN.md` T6.

Migration v1:
- `events(id, ts, session_id, kind, event_json, raw_json)` — append-only, xóa sau `event_ttl_days` (mặc định 30)
- `approvals(request_id, session_id, agent_id, tool, command_original, command_normalized, risk, group_key, status,
  decided_by, created_at, decided_at, latency_ms, channel)` — `status`: `pending | allowed | denied | timed_out | cancelled |
  handed_off`; `command_*` là đúng phần người duyệt thấy trên thẻ

Migration v2 (T9): `batch_rules(group_key, opened_at, expires_at)`. Migration v3 (T10):
- `trust_grants(id, subject, pattern, level, granted_at, expires_at, reason, revoked_at)` — `subject` là tên persona,
  `pattern` là `group_key`
- `streaks(subject, pattern, approved_in_row, first_at, last_at, last_incident_at)` — để đề xuất nâng/hạ mức
- `trust_log(id, ts, subject, pattern, from_level, to_level, reason)`

Persona không lưu trong SQLite: đọc từ `arcturus.toml` khi hub khởi động.

## WebSocket (chi tiết: docs/PROTOCOL.md)

Mỗi thông điệp là JSON `{"t": "<loại>", ...}`; kiểu Rust ở `docs/PLAN.md` §3.4. Xác thực bằng **token UI riêng** (hub
sinh mỗi lần chạy, ghi `<data_dir>/arcturus/ui-token`), không phải token của hook; `Host` loopback, `Origin` Tauri/Vite.

Server → client: `snapshot {agents, queue, handoffs, team}`, `agent_updated {agent}`, `agent_removed {id}`,
`approval_requested {approval}`, `approval_resolved {request_id, decision | null, decided_by}`, `team_updated {team}`,
`feed {text, ts}`, `approval_handed_off {handoff}`, `handoff_cleared {handoff_id}`, `suggestion {suggestion}` (T10).

Client → server: `decide {request_id, decision, message?, origin: user | countdown}`, `decide_group {group_key, decision}` (T9),
`batch_open {group_key, minutes}` (T9), `batch_close {group_key}` (T9), `team_pause`, `team_resume`,
`focus_set {on}` (T14), `suggestion_answer {id, accept}` (T10). Không có `return_to_terminal` (Q1: không cần).

Lệnh quá dài để duyệt trên thẻ (mặc định > 4.096 byte hoặc > 50 dòng, trừ Edit/Write/MultiEdit/NotebookEdit vốn cắt
nội dung kèm bản đầy đủ): hub trả `{}` ngay để người duyệt trong terminal, chỉ broadcast thẻ báo `approval_handed_off`
không có nút duyệt (quy tắc bất biến 3).

## Giới hạn bảo mật đã biết (v0)

Arcturus giúp người dùng duyệt yêu cầu của agent; nó **không phải sandbox**. Một agent đã được chạy mã tùy ý dưới cùng
tài khoản người dùng (lệnh đã allow sẵn trong settings, hoặc người dùng đã duyệt) làm được gần như mọi thứ người dùng làm
được, và nằm ngoài phạm vi bảo vệ của v0. Cụ thể (F6, `docs/HOOK_FINDINGS.md`):

- Bash tool thừa hưởng biến môi trường của tiến trình `claude`, nên đọc được `ARCTURUS_TOKEN` (token của hook). Vì vậy
  UI dùng token riêng (D1).
- File token UI (`<data_dir>/arcturus/ui-token`) cũng đọc được bởi agent như vậy. Giảm rủi ro (T5d · ARC-44):
  `--ui-token-stdout` để app nhận token qua pipe, không có file trên đĩa (B1; app T12 dùng chế độ này khi tự khởi
  động hub; hub chạy riêng vẫn ghi file); luật danger `arcturus_ui_token` đưa mọi lệnh chạm token hay WebSocket của hub về
  Critical (B2, đã có ở T8 · ARC-29, cùng luật `arcturus_state` cho file cấu hình và cơ sở dữ liệu của hub); sàn thời gian duyệt `min_review_ms` chặn tự duyệt ngay khi thẻ vừa được
  phát, áp cho mọi allow kể cả đếm ngược (B4, đã có).
- Còn lại: agent đọc được bộ nhớ của tiến trình app/hub cùng tài khoản, hoặc chờ quá sàn thời gian rồi gửi `decide`,
  vẫn có thể tự duyệt. Lớp bảo vệ thật là deny rule trong settings của Claude Code (hook allow không vượt được deny) và
  việc người dùng không allow sẵn lệnh chạy mã tùy ý.

## Mở rộng sau v0

- v1: VRM view (three-vrm, chỉ render khi cửa sổ hiện), **trợ lý 3D "Pet" kiêm orchestrator** (overlay trong suốt, tương tác; đề xuất ở `docs/PLAN.md` §7), giọng nói (whisper.cpp base, Piper), trust ladder 7 mức.
- v2: adapter Codex (hooks.json, PermissionRequest), OpenCode (plugin), quan sát Gemini/Cursor; `arcturus-agent` trên máy
  remote nối về hub qua WebSocket/MQTT; PWA mobile.
- v3: thiết bị đồng hành **Izar** (sản phẩm riêng, project IZAR) — ESP32-S3: BLE (tương thích wire protocol Hardware Buddy) hoặc MQTT.

## Giao diện

Các lớp UI, điểm thay thế (bộ vẽ nhân vật, cảnh Văn phòng, token kiểu dáng), luật chuyển động và danh sách kiểm cho PR đổi
giao diện: `docs/UI_EXTENSION.md`.
