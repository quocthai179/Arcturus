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
3. Policy quyết theo thứ tự:
   1. Khớp **danger list** → luôn hỏi người (không bao giờ tự allow).
   2. Có **trust grant** còn hạn cho (agent hoặc role) × (mẫu lệnh) ở mức ≥6 → tự allow, ghi log `decided_by=policy`.
   3. Có **batch rule** đang mở ("cho phép `npm test` 1 giờ") → tự allow.
   4. Còn lại → vào hàng đợi.
4. Vào hàng đợi: tạo `oneshot` channel, broadcast `approval.requested` qua WebSocket.
5. Người quyết qua UI → hub gửi vào channel → trả JSON decision cho Claude Code.
6. Hết 110 giây (timeout hook = 120) → trả `{}`. Claude Code tự hỏi trong terminal. Đánh dấu `decided_by=timeout`.
7. Mọi quyết định ghi vào bảng `approvals` (ai, lúc nào, qua thiết bị nào, độ trễ).

Sơ đồ trạng thái một yêu cầu: `pending → allowed | denied | timed_out | cancelled(session_ended)`.

## Suy ra trạng thái agent (0 token)

| Sự kiện | activity | mood nhân vật |
|---|---|---|
| SessionStart | idle | idle |
| UserPromptSubmit | thinking | working |
| PreToolUse Edit/Write/MultiEdit | typing | working |
| PreToolUse Read/Grep/Glob | reading | working |
| PreToolUse Bash | running | working |
| PermissionRequest | waiting | approval |
| PostToolUseFailure, StopFailure | error | error |
| Stop | idle | done (3 giây) → idle |
| SubagentStart/Stop | tạo/xóa agent con `kind=subagent` | |
| SessionEnd | xóa agent | |

## Định danh và persona

- Khóa agent: `session_id`. Agent con: `session_id + agent_id` từ SubagentStart.
- Persona gán qua `arcturus.toml` theo `cwd` hoặc tên phiên:

```toml
[[persona]]
match_cwd = "**/worktrees/ui*"
name = "Linh"
role = "frontend"
palette = { hair = "#2E2A5C", outfit = "#8B7CF6", acc = "#FF7A59" }
```

Không khớp: tự đặt tên từ danh sách mặc định, màu theo hash của session_id.

## Schema sự kiện chung (v1)

```json
{
  "v": 1,
  "id": "evt_01J...",
  "ts": "2026-10-21T09:15:02.123Z",
  "host": "laptop",
  "source": "claude-code",
  "session_id": "abc123",
  "agent": { "id": "agt_abc123", "name": "Linh", "role": "frontend", "parent_id": null, "kind": "session" },
  "type": "session.started | activity | tool.pre | tool.post | approval.requested | approval.resolved | task.created | task.completed | agent.idle | agent.error | session.ended",
  "activity": "typing | reading | running | thinking | waiting | idle",
  "tool": { "name": "Bash", "summary": "npm test", "risk": "low | medium | high | critical" },
  "approval": { "request_id": "req_x", "deadline_ms": 110000, "decided_by": "user | policy | timeout", "decision": "allow | deny" },
  "metrics": { "context_pct": 0.62 },
  "raw_ref": "data/events/abc123/000042"
}
```

`raw_ref` trỏ tới JSON gốc lưu cục bộ. Renderer chỉ nhận tóm tắt, không nhận diff hay nội dung file.

## Bảng SQLite

- `events(id, ts, session_id, type, payload_json)` — append-only, xóa sau 30 ngày
- `approvals(request_id, session_id, agent_id, tool, input_json, risk, status, decided_by, decided_at, latency_ms, channel)`
- `trust_grants(id, subject, subject_kind, pattern, level, granted_at, expires_at, reason)`
- `streaks(subject, pattern, approved_in_row, last_incident_at)` — để đề xuất nâng/hạ mức
- `personas` — cache từ `arcturus.toml`

## WebSocket (chi tiết viết ở docs/PROTOCOL.md khi làm T5)

Server → client: `snapshot`, `agent.updated`, `approval.requested`, `approval.resolved`, `feed`.
Client → server: `approval.decide {request_id, decision, scope?}`, `batch.open {pattern, minutes}`,
`team.pause`, `team.resume`, `focus.set {on}`.

## Mở rộng sau v0

- v1: VRM view (three-vrm, chỉ render khi cửa sổ hiện), giọng nói (whisper.cpp base, Piper), trust ladder 7 mức.
- v2: adapter Codex (hooks.json, PermissionRequest), OpenCode (plugin), quan sát Gemini/Cursor; `arcturus-agent` trên máy
  remote nối về hub qua WebSocket/MQTT; PWA mobile.
- v3: thiết bị đồng hành **Izar** (sản phẩm riêng, project IZAR) — ESP32-S3: BLE (tương thích wire protocol Hardware Buddy) hoặc MQTT.
