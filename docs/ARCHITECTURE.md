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

Hub là daemon riêng. Đóng app thì hub vẫn chạy và vẫn giữ hàng đợi. App chỉ là client.

## Luồng duyệt (PermissionRequest)

1. Claude Code gửi POST `/hooks/permission` với JSON đầu vào của hook.
2. Hub chuẩn hóa thành `Event{type: approval.requested}`, tính `risk` bằng policy (không tin agent tự khai).
3. Policy quyết theo thứ tự (chi tiết: `docs/PLAN.md` T3, T8–T10, T14):
   1. Khớp **danger list** (risk Critical) → luôn hỏi người (không bao giờ tự allow).
   2. Có **trust grant** còn hạn cho (persona) × (`group_key`) ở mức 6 và risk ≤ Medium → tự allow, ghi log
      `decided_by=policy`. Mức 5 → vẫn hỏi người, kèm đếm ngược phủ quyết.
   3. Có **batch rule** đang mở cho `group_key` ("cho phép `Bash:npm test` 1 giờ") và risk ≤ Medium → tự allow.
   4. Còn lại → vào hàng đợi.

   Đội đang **tạm dừng** (`team.paused`): mọi kết quả tự allow ở bước 2–3 bị đổi thành hỏi người. Tạm dừng không
   tự từ chối và ở v0 không dừng được agent thật (xem T14).
4. Vào hàng đợi: tạo `oneshot` channel, broadcast `approval.requested` qua WebSocket.
5. Người quyết qua UI → hub gửi vào channel → trả JSON decision cho Claude Code.
6. Hết 110 giây (timeout hook = 120) → trả `{}`. Claude Code tự hỏi trong terminal. Đánh dấu `decided_by=timeout`.
7. Mọi quyết định ghi vào bảng `approvals` (ai, lúc nào, qua thiết bị nào, độ trễ).

Sơ đồ trạng thái một yêu cầu: `pending → allowed | denied | timed_out | cancelled(session_ended)`.

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
| PostToolUseFailure, StopFailure | error | error |
| Stop | idle | done (3 giây) → idle |
| SubagentStart/Stop | tạo/xóa agent con `kind=subagent` | |
| SessionEnd | xóa agent | |

## Định danh và persona

- Khóa agent: `session_id` → `agt_<session_id>`. Agent con: `session_id + agent_id` từ SubagentStart →
  `agt_<session_id>_<agent_id>`.
- Persona gán qua `arcturus.toml` theo `cwd` (glob `match_cwd`, `docs/PLAN.md` §3.7):

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
  "agent": { "id": "agt_abc123", "parent_id": null, "kind": "session | subagent | teammate" },
  "type": "session.started | prompt.submitted | tool.pre | tool.post | tool.failed | approval.requested | approval.resolved | subagent.started | subagent.stopped | notification | turn.stopped | turn.failed | session.ended | unknown",
  "activity": "typing | reading | running | thinking | waiting | idle",
  "tool": { "name": "Bash", "summary": "npm test", "risk": "low | medium | high | critical" },
  "approval": { "request_id": "req_01J...", "deadline_ms": 110000, "decided_by": "user | policy | timeout | cancelled", "decision": "allow | deny" },
  "metrics": { "context_pct": 0.62 },
  "raw_ref": "evt_01J..."
}
```

- `activity`, `tool`, `approval`, `metrics`, `raw_ref` có thể là `null`; `tool.risk` chỉ policy được điền.
- `type: "unknown"`: sự kiện hook lạ được giữ lại, không bao giờ làm hub trả lỗi.
- Tên, vai trò, bảng màu không nằm trong Event: hub gắn persona khi dựng `AgentView` (`docs/PLAN.md` §3.4).
- `raw_ref` là khóa tới JSON hook gốc trong SQLite (`events.raw_json`). UI chỉ nhận `AgentView`/`ApprovalView`
  (tóm tắt hoạt động và lệnh cần duyệt), không nhận diff hay nội dung file.

## Bảng SQLite

DDL chính xác: `docs/PLAN.md` §3.5. Migration đánh số bằng `PRAGMA user_version`.

Migration v1:
- `events(id, ts, session_id, kind, event_json, raw_json)` — append-only, xóa sau `event_ttl_days` (mặc định 30)
- `approvals(request_id, session_id, agent_id, tool, command_original, command_normalized, risk, group_key, status,
  decided_by, created_at, decided_at, latency_ms, channel)` — `status`: `pending | allowed | denied | timed_out | cancelled`

Migration v2 (T9, T10):
- `batch_rules(group_key, opened_at, expires_at)`
- `trust_grants(id, subject, pattern, level, granted_at, expires_at, reason, revoked_at)` — `subject` là tên persona,
  `pattern` là `group_key`
- `streaks(subject, pattern, approved_in_row, first_at, last_at, last_incident_at)` — để đề xuất nâng/hạ mức
- `trust_log(id, ts, subject, pattern, from_level, to_level, reason)`

Persona không lưu trong SQLite: đọc từ `arcturus.toml` khi hub khởi động.

## WebSocket (chi tiết viết ở docs/PROTOCOL.md khi làm T5)

Mỗi thông điệp là JSON `{"t": "<loại>", ...}`; kiểu Rust ở `docs/PLAN.md` §3.4.

Server → client: `snapshot {agents, queue, team}`, `agent_updated {agent}`, `agent_removed {id}`,
`approval_requested {approval}`, `approval_resolved {request_id, decision, decided_by}`, `team_updated {team}`,
`feed {text, ts}`, `suggestion {suggestion}` (T10).

Client → server: `decide {request_id, decision, message?}`, `decide_group {group_key, decision}` (T9),
`batch_open {group_key, minutes}` (T9), `batch_close {group_key}` (T9), `team_pause`, `team_resume`,
`focus_set {on}` (T14), `suggestion_answer {id, accept}` (T10), `return_to_terminal {request_id}` (nếu Q1 cần).

## Mở rộng sau v0

- v1: VRM view (three-vrm, chỉ render khi cửa sổ hiện), giọng nói (whisper.cpp base, Piper), trust ladder 7 mức.
- v2: adapter Codex (hooks.json, PermissionRequest), OpenCode (plugin), quan sát Gemini/Cursor; `arcturus-agent` trên máy
  remote nối về hub qua WebSocket/MQTT; PWA mobile.
- v3: thiết bị đồng hành **Izar** (sản phẩm riêng, project IZAR) — ESP32-S3: BLE (tương thích wire protocol Hardware Buddy) hoặc MQTT.
