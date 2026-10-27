# Arcturus v0 — Kế hoạch triển khai

Tài liệu này dành cho Claude Code. Mỗi task (T0…T21, mã ARC trong `.squad/TASKS.md`) có một mục riêng ở phần 6
với: mục tiêu, file tạo/sửa, thiết kế chi tiết, test bắt buộc, lệnh kiểm tra.

**Cách dùng**
1. Đọc phần 1–5 một lần (hợp đồng chung, áp dụng cho mọi task).
2. Khi nhận task Tn: đọc mục `### Tn` ở phần 6, dùng nó làm khung cho kế hoạch (plan mode) rồi code.
3. Gặp mâu thuẫn giữa file này và hành vi thật của Claude Code (ghi trong `docs/HOOK_FINDINGS.md`): **hành vi thật thắng**.
   Ghi rõ chỗ lệch vào báo cáo cuối và đề xuất sửa file này; không tự sửa phần hợp đồng chung (phần 3) nếu chưa có người duyệt.
4. Phiên bản thư viện: luôn thêm bằng `cargo add` / `pnpm add` để lấy bản ổn định mới nhất. Không chép số phiên bản từ trí nhớ.

Quy tắc bất biến và định nghĩa "xong" nằm ở `CLAUDE.md` và áp dụng cho mọi task ở đây.

---

## 1. Cấu trúc repo khi xong v0

```
arcturus/
├── Cargo.toml                    # [workspace] members = hub-core, hub, tools/replay ; exclude crates/app ; resolver = "2"
├── crates/
│   ├── hub-core/                 # thư viện thuần, không I/O mạng
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── schema.rs         # Event v1, Agent, Approval… (nguồn sự thật)
│   │       ├── hook.rs           # JSON hook → HookInput (chịu lỗi) → Event
│   │       ├── state.rs          # TeamState: agents, sessions, state machine
│   │       ├── persona.rs        # arcturus.toml → Persona, khớp theo cwd, tên mặc định
│   │       ├── config.rs         # Config (đọc arcturus.toml)
│   │       ├── protocol.rs       # ServerMsg / ClientMsg (WebSocket)
│   │       ├── store.rs          # SQLite: migrations, ghi/đọc, dọn TTL
│   │       ├── standup.rs        # (T19) tổng hợp standup
│   │       └── policy/
│   │           ├── mod.rs        # Policy::decide(&ApprovalCtx) -> PolicyOutcome
│   │           ├── normalize.rs  # chuẩn hóa lệnh
│   │           ├── danger.rs     # vùng khóa
│   │           ├── risk.rs       # phân loại rủi ro
│   │           ├── batch.rs      # gom nhóm + batch rule
│   │           └── trust.rs      # trust grant + streak
│   ├── hub/                      # binary daemon
│   │   └── src/
│   │       ├── main.rs           # đọc config, mở store, chạy server
│   │       ├── server.rs         # axum Router
│   │       ├── auth.rs           # kiểm Bearer token
│   │       ├── hooks.rs          # /hooks/event, /hooks/permission
│   │       ├── approvals.rs      # PendingApprovals (oneshot), timeout
│   │       ├── ws.rs             # /ws
│   │       ├── reconcile.rs      # (T7) claude agents --json
│   │       └── hub.rs            # struct Hub (Arc), broadcast, state lock
│   └── app/                      # Tauri 2 (src-tauri bên trong)
├── ui/                           # Vite + React + TS
│   └── src/
│       ├── main.tsx
│       ├── protocol.ts           # kiểu khớp protocol.rs
│       ├── ws.ts                 # client WebSocket, tự kết nối lại
│       ├── store.ts              # useReducer: snapshot + delta
│       ├── components/
│       │   ├── Character.tsx     # SVG nhân vật (phụ lục A)
│       │   ├── ApprovalCard.tsx
│       │   ├── QueueList.tsx
│       │   ├── AgentDesk.tsx
│       │   └── TeamBar.tsx
│       └── views/
│           ├── LitePopup.tsx
│           └── Office.tsx
├── tools/replay/                 # binary replay
├── fixtures/hooks/<SựKiện>/*.json
├── hooks/arcturus-dogfood.json
├── arcturus.example.toml
└── docs/  (ARCHITECTURE, PLAN, PROTOCOL, BUDGET, HOOK_FINDINGS, WORKFLOW)
```

## 2. Thư viện

**Rust** (thêm bằng `cargo add`):
| Crate | Dùng cho | Ghi chú |
|---|---|---|
| tokio | runtime | features `rt-multi-thread`, `macros`, `sync`, `time`, `process`, `signal` |
| axum | HTTP + WebSocket | feature `ws` |
| serde, serde_json | JSON | `derive` |
| thiserror | lỗi trong hub-core | |
| anyhow | lỗi trong binary | chỉ ở `hub`, `replay` |
| tracing, tracing-subscriber | log | `env-filter`; mặc định `info`, không log nội dung lệnh ở mức info |
| rusqlite | SQLite | feature `bundled` |
| ulid | id sự kiện, request | |
| time | thời gian RFC 3339 | features `serde`, `formatting`, `parsing` |
| toml | đọc config | |
| globset | khớp `match_cwd` | |
| regex | danger list | biên dịch một lần (`std::sync::LazyLock`) |
| dirs | thư mục dữ liệu | |
| clap | CLI của `hub`, `replay` | `derive` |
| reqwest | replay gửi HTTP | `rustls-tls`, `json`; tắt default features |
| **dev:** insta | snapshot test | feature `json` |
| **dev:** tokio-tungstenite | test WebSocket | |

**UI** (`pnpm -C ui add`): react, react-dom; dev: vite, @vitejs/plugin-react, typescript, vitest, jsdom,
@testing-library/react, @testing-library/user-event.

**App**: Tauri 2 (`@tauri-apps/cli`, crate `tauri` với feature tray icon theo tài liệu Tauri 2 hiện hành).

Không thêm thư viện khác nếu chưa ghi lý do trong báo cáo cuối của task.

## 3. Hợp đồng chung (không đổi nếu chưa có người duyệt)

### 3.1 Schema (`hub-core/src/schema.rs`)

```rust
pub const SCHEMA_VERSION: u32 = 1;

pub type AgentId = String;      // "agt_<session_id>" hoặc "agt_<session_id>_<agent_id>"
pub type RequestId = String;    // "req_<ulid>"

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    pub v: u32,
    pub id: String,                 // "evt_<ulid>"
    pub ts: OffsetDateTime,         // RFC 3339
    pub host: String,               // hostname
    pub source: Source,             // ClaudeCode (v0)
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<String>,        // `cwd` của hook (T4: persona, AgentView.cwd); thiếu → None, không tăng v
    pub agent: AgentRef,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub activity: Option<Activity>,
    pub tool: Option<ToolInfo>,
    pub approval: Option<ApprovalInfo>, // hub điền khi tạo yêu cầu duyệt (T3); hook::to_event luôn để None
    pub metrics: Option<Metrics>,
    // Không có raw_ref: JSON hook gốc nằm ở cột events.raw_json, tra theo `id` (§3.5).
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Source { ClaudeCode }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentRef {
    pub id: AgentId, pub parent_id: Option<AgentId>, pub kind: AgentKind,
    #[serde(default)]
    pub agent_type: Option<String>, // subagent: "general-purpose"… (Q4), vai trò nhân vật (T4); thiếu → None
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind { Session, Subagent, Teammate }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    #[serde(rename = "session.started")] SessionStarted,
    #[serde(rename = "prompt.submitted")] PromptSubmitted,
    #[serde(rename = "tool.pre")] ToolPre,
    #[serde(rename = "tool.post")] ToolPost,
    #[serde(rename = "tool.failed")] ToolFailed,
    #[serde(rename = "approval.requested")] ApprovalRequested,
    #[serde(rename = "approval.resolved")] ApprovalResolved,
    #[serde(rename = "subagent.started")] SubagentStarted,
    #[serde(rename = "subagent.stopped")] SubagentStopped,
    #[serde(rename = "notification")] Notification,
    #[serde(rename = "turn.stopped")] TurnStopped,
    #[serde(rename = "turn.failed")] TurnFailed,
    #[serde(rename = "session.ended")] SessionEnded,
    #[serde(rename = "unknown")] Unknown,   // sự kiện chưa hỗ trợ: giữ lại, không lỗi
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Activity { Typing, Reading, Running, Thinking, Waiting, Idle }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mood { Idle, Working, Approval, Error, Done }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Risk { Low, Medium, High, Critical }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolInfo {
    pub name: String,               // "Bash", "Edit", "Read", …
    pub summary: String,            // lệnh/đường dẫn rút gọn, ≤200 ký tự Unicode (cắt kèm "…") cho UI; không dùng để duyệt
    pub risk: Option<Risk>,         // chỉ policy được điền
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ApprovalInfo {
    pub request_id: RequestId,
    pub deadline_ms: u64,
    pub decided_by: Option<DecidedBy>,
    pub decision: Option<Decision>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision { Allow, Deny }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy { User, Countdown, Policy, Timeout, Cancelled }   // Countdown: hết giờ đếm ngược phủ quyết (T10, T14)

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Metrics { pub context_pct: Option<f32> }
```

`Unknown` là bắt buộc: sự kiện hook mới hoặc lạ **không bao giờ** làm hub trả lỗi.

### 3.2 Đọc JSON hook (`hub-core/src/hook.rs`)

Hình dạng chính xác chốt ở T1 từ fixture. Thiết kế đọc chịu lỗi:

```rust
pub struct HookInput { pub raw: serde_json::Value }

impl HookInput {
    pub fn event_name(&self) -> &str;          // "hook_event_name"
    pub fn session_id(&self) -> Option<&str>;  // "session_id"
    pub fn cwd(&self) -> Option<&str>;         // "cwd"
    pub fn tool_name(&self) -> Option<&str>;   // "tool_name"
    pub fn tool_input(&self) -> Option<&serde_json::Value>; // "tool_input"
    pub fn agent_id(&self) -> Option<&str>;    // "agent_id": có ở MỌI sự kiện của subagent (Q4, T1)
    pub fn agent_type(&self) -> Option<&str>;  // "agent_type", ví dụ "general-purpose" (Q4)
}

pub fn to_event(input: &HookInput, host: &str, now: OffsetDateTime) -> Event;
pub fn tool_summary(tool_name: &str, tool_input: &serde_json::Value) -> String;
```

- Thiếu `session_id`: dùng `"unknown"`, vẫn tạo Event (ghi `tracing::warn`).
- `tool_summary`: Bash → `command`; Edit/Write/Read/MultiEdit → `file_path`; NotebookEdit → `notebook_path`;
  Grep/Glob → `pattern`; khác hoặc thiếu trường → tên tool. Cắt còn ≤200 ký tự Unicode.
- `agent`: có `agent_id` → `AgentRef{id: "agt_<session>_<agent_id>", parent_id: "agt_<session>", kind: subagent}`;
  không có → `AgentRef{id: "agt_<session>", parent_id: None, kind: session}`.
- `approval` luôn `None`, `metrics` luôn `None` (hook chưa gửi dữ liệu context), `tool.risk` luôn `None`.
- Ánh xạ `hook_event_name` → `EventKind`:

| hook_event_name | EventKind | activity |
|---|---|---|
| SessionStart | SessionStarted | idle |
| UserPromptSubmit | PromptSubmitted | thinking |
| PreToolUse | ToolPre | theo tool (bảng 3.6) |
| PostToolUse | ToolPost | thinking |
| PostToolUseFailure | ToolFailed | — |
| PermissionRequest | ApprovalRequested | waiting |
| SubagentStart / SubagentStop | SubagentStarted / SubagentStopped | — |
| Notification | Notification | — |
| Stop / StopFailure | TurnStopped / TurnFailed | idle / — |
| SessionEnd | SessionEnded | — |
| khác | Unknown | — |

### 3.3 HTTP (`hub`)

Bind mặc định `127.0.0.1:7878` (cấu hình được nhưng **từ chối** địa chỉ không phải loopback trong v0).

| Phương thức | Đường dẫn | Xác thực | Phản hồi |
|---|---|---|---|
| POST | `/hooks/event` | Bearer token hook | `204` ngay; xử lý bất đồng bộ qua `mpsc` |
| POST | `/hooks/permission` | Bearer token hook | `200` + JSON quyết định, hoặc `200 {}` |
| GET | `/ws` | token UI: Bearer (header) hoặc `?token=` (cho webview) | Nâng cấp WebSocket |
| GET | `/approvals/{request_id}/input` | token UI: chỉ Bearer (header) | `200` + `tool_input` đầy đủ của yêu cầu đang chờ (nút "Xem đầy đủ", T12), `nosniff`, `no-store`; `404` nếu không còn chờ hoặc là thẻ duyệt-trong-terminal |
| GET | `/healthz` | không | `200 ok` |

- Token hook: biến môi trường `ARCTURUS_TOKEN`; không có thì hub **không khởi động** (thông báo cách tạo).
- Token UI (D1, T5): hub tự sinh 32 byte ngẫu nhiên mỗi lần chạy, ghi `<data_dir>/arcturus/ui-token` (`--ui-token-file`
  để đổi); app đọc file đó. Không nằm trong biến môi trường của Claude Code (Bash tool thừa hưởng `ARCTURUS_TOKEN`,
  `docs/HOOK_FINDINGS.md` F6). `/ws`, `/approvals/*` chỉ nhận token UI; `/hooks/*` chỉ nhận token hook. Không log token.
- `--ui-token-stdout` (T5d · ARC-44, F6 biện pháp B1): hub in token UI **đúng một dòng** ra stdout rồi flush, **không ghi
  file**; log chuyển sang stderr. Dành cho app khởi động hub làm tiến trình con và đọc token qua pipe (T12), để agent không
  đọc được token từ đĩa. Xung đột với `--record`, `--ui-token-file`. Mặc định vẫn ghi file tới khi T12 xong.
- Sàn thời gian duyệt (T5d · ARC-44, F6 biện pháp B4): mọi `decide{allow}` (cả `origin: user` lẫn `countdown`) tới sớm
  hơn `min_review_ms` (mặc định 1.000 ms) kể từ lúc thẻ được **phát** (broadcast `approval_requested`) thì bị **giữ lại**
  (`HeldReason::TooFast`, `feed` "Duyệt quá nhanh: xem lại thẻ rồi bấm lại"), yêu cầu vẫn chờ. Không áp cho `deny`.
  `0` là tắt. Đếm ngược cũng phải qua sàn vì `origin: countdown` do client khai, hub không biết có ai nhìn thấy thẻ (audit
  ARC-44 finding 4). Mọi điều kiện nhận quyết định nằm trong `approvals::allow_gate`; duyệt gộp/batch (T9) phải gọi nó cho
  từng yêu cầu. Giới hạn đã chấp nhận khi duyệt: script đọc được token chỉ cần chờ là vượt qua; người duyệt nhanh tay sẽ
  phải bấm lại; thẻ tới UI muộn qua `Snapshot` (popup mở sau) không được sàn bảo vệ, nên UI cũng tính sàn từ lúc hiện thẻ.
- `/ws`, `/approvals/*`: `Host` phải là `127.0.0.1:<cổng>`, `localhost:<cổng>` hoặc `[::1]:<cổng>` (chặn DNS rebinding);
  `Origin`, nếu có, phải thuộc webview Tauri hoặc Vite dev (`ws::ALLOWED_ORIGINS`). Sai → `403`.
- So sánh token bằng so sánh thời gian hằng (constant-time).
- `/hooks/permission` sai token: trả `401`. Claude Code coi đó là lỗi không chặn → hỏi trong terminal (an toàn).
- Body tối đa 1 MB; JSON hỏng → `400` (vẫn là lỗi không chặn với Claude Code).
- Phản hồi quyết định:

```json
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Trưởng nhóm từ chối: <lý do nếu có>"}}}
```

Trường `message` khi deny: xác nhận ở Q6; nếu Claude Code không đọc thì bỏ, không lỗi.

### 3.4 WebSocket (`hub-core/src/protocol.rs`, mô tả ở `docs/PROTOCOL.md`)

JSON, `{"t": "<loại>", ...}`.

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Snapshot { agents: Vec<AgentView>, queue: Vec<ApprovalView>, handoffs: Vec<HandoffView>, team: TeamView },
    AgentUpdated { agent: AgentView },
    AgentRemoved { id: AgentId },
    ApprovalRequested { approval: ApprovalView },
    ApprovalResolved { request_id: RequestId, decision: Option<Decision>, decided_by: DecidedBy }, // None khi timeout/cancelled (D2, T5)
    TeamUpdated { team: TeamView },
    Feed { text: String, ts: OffsetDateTime },
    ApprovalHandedOff { handoff: HandoffView },       // lệnh quá dài: duyệt trong terminal (T5b)
    HandoffCleared { handoff_id: String },            // T5b
    Suggestion { suggestion: TrustSuggestionView },   // T10
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Decide { request_id: RequestId, decision: Decision, message: Option<String>, origin: DecideOrigin },
    DecideGroup { group_key: String, decision: Decision },          // T9
    BatchOpen { group_key: String, minutes: u32 },                  // T9
    BatchClose { group_key: String },                               // T9
    TeamPause, TeamResume,                                          // hub: T5; UI: T14
    FocusSet { on: bool },                                          // T14
    SuggestionAnswer { id: String, accept: bool },                  // T10
    // Không có ReturnToTerminal: terminal luôn hỏi song song (Q1); lệnh quá dài hub tự trả về terminal (T5b).
}

pub struct HandoffView {                               // thẻ báo, KHÔNG có nút duyệt
    pub handoff_id: String,                            // "hof_<ulid>", không bao giờ là request_id đang chờ
    pub agent_id: AgentId, pub agent_name: String, pub tool: String,
    pub bytes: usize, pub lines: usize,                // kích thước lệnh đầy đủ
    pub preview_head: String, pub preview_tail: String, // ~300 ký tự mỗi đầu, ký tự ẩn đã hiện ⟨U+XXXX⟩
    pub suspicions: Vec<Suspicion>,                    // tính trên lệnh ĐẦY ĐỦ
    pub message: String, pub created_at: OffsetDateTime,
}

pub struct AgentView {
    pub id: AgentId, pub parent_id: Option<AgentId>, pub kind: AgentKind,
    pub name: String, pub role: String, pub palette: Palette,
    pub mood: Mood, pub activity: Activity, pub act_text: String,   // "Đang sửa LoginForm.tsx"
    pub host: String, pub source: Source, pub cwd: String,
    pub context_pct: Option<f32>, pub last_event_at: OffsetDateTime,
}

pub struct ApprovalView {
    pub request_id: RequestId, pub agent_id: AgentId, pub agent_name: String,
    pub tool: String, pub command_original: String,   // tool_input gốc dạng chuỗi (giới hạn nội dung: xem dưới)
    pub command_normalized: String,                    // sau normalize
    pub danger_spans: Vec<(usize, usize)>,             // [start, end) theo chỉ số ký tự Unicode trong command_normalized
    pub suspicions: Vec<Suspicion>,                    // từ normalize (T8): hidden_chars, subshell, eval, encoded
    pub truncated: bool,                               // true nếu nội dung Edit/Write bị cắt (xem dưới)
    pub risk: Risk, pub reversible: bool,
    pub group_key: String, pub created_at: OffsetDateTime, pub deadline_at: OffsetDateTime,
    pub countdown_ms: Option<u64>,                     // đếm ngược phủ quyết (T10 mức 5); None khi đội tạm dừng
}

#[serde(rename_all = "snake_case")]
pub enum DecideOrigin { User, Countdown }       // người bấm, hay UI tự gửi khi hết đếm ngược

pub struct TeamView { pub paused: bool, pub focus: bool, pub roster_max: u8 }
pub struct Palette { pub hair: String, pub outfit: String, pub acc: String }
```

Quy tắc: client mới kết nối luôn nhận `Snapshot` trước. Mọi thay đổi sau đó là delta. Client không bao giờ phải
tự suy luận trạng thái.

**Đơn vị của `danger_spans`**: khoảng nửa mở `[start, end)` tính bằng chỉ số **ký tự Unicode** (scalar value,
tức code point) trong `command_normalized`; không phải byte UTF-8, không phải UTF-16 code unit.
- Hub: `regex::Match` trả offset byte → đổi bằng `text[..byte].chars().count()` trước khi gửi.
- UI: cắt bằng `Array.from(command_normalized).slice(start, end).join('')`, không dùng `String.prototype.slice`.
- Lý do: lệnh có thể chứa đường dẫn tiếng Việt, emoji hoặc ký tự ngoài BMP; byte và UTF-16 lệch nhau ở các ký tự này
  và làm tô sáng sai chỗ.

**Giới hạn nội dung trong `ApprovalView`** (giữ WebSocket và popup trong ngân sách, quy tắc bất biến 6):
- Bash, tool khác ngoài nhóm dưới đây, và mọi trường đường dẫn (`file_path`, `path`, `pattern`) **không bao giờ bị cắt**
  (quy tắc bất biến 3).
- Chỉ với Edit/Write/MultiEdit/NotebookEdit: trường nội dung (`content`, `old_string`, `new_string`, `new_source`; MultiEdit:
  từng edit, giữ mảng và `replace_all`) bị cắt còn 2.000 ký tự Unicode mỗi trường, kèm dòng `… [đã cắt, tổng N ký tự]`;
  khi đó `truncated = true` và UI bắt mở bản đầy đủ trước khi Allow (T12).
- Luật danger cho các tool này chỉ khớp trên đường dẫn (T8), nên `danger_spans` luôn nằm trong phần không bị cắt.
- Bản đầy đủ lấy theo yêu cầu qua `GET /approvals/{request_id}/input` (§3.3), không gửi qua broadcast.

**Lệnh quá dài: trả về terminal** (quyết định của người duyệt ở T5b; quy tắc bất biến 3: "Hub không bao giờ cắt bớt một
lệnh mà nó cho người duyệt; lệnh quá dài để xem trên thẻ thì trả về terminal"):
- Áp dụng cho mọi tool **không** thuộc nhóm cắt được ở trên (Bash, MCP…): nội dung thẻ dài hơn `handoff_max_bytes`
  (mặc định 4.096 byte UTF-8) hoặc `handoff_max_lines` (mặc định 50 dòng), cấu hình ở `[hub]` (§3.7).
- Hub kiểm **trước** policy, trả `{}` ngay (Claude Code hỏi trong terminal, hộp thoại hiện đầy đủ lệnh và chờ, Q2),
  không giữ yêu cầu, không bao giờ allow/deny, không tự duyệt, không đếm ngược. Ghi `approvals.status = handed_off`.
- Broadcast `ApprovalHandedOff{HandoffView}`: thẻ báo không có nút; xem trước đầu/cuối; cờ tính trên lệnh đầy đủ;
  "Lệnh quá dài để duyệt ở đây — hãy mở terminal của <agent> để xem đủ và quyết định." Nhân vật: chờ duyệt, chữ
  "Duyệt trong terminal".
- `Decide` với `handoff_id` → `Feed` "Yêu cầu này đang được duyệt trong terminal"; `GET /approvals/{id}/input` → `404`.
- Thẻ bị xóa (`HandoffCleared`) theo đúng luật hủy yêu cầu "đã được trả lời ở nơi khác" của T3 (sự kiện kế tiếp của cùng
  agent; `PostToolUse` khớp bằng hash `tool_input`) hoặc `SessionEnd`. Tối đa 64 thẻ, đầy thì bỏ thẻ cũ nhất.

**Đếm ngược phủ quyết và tạm dừng** (bất biến: khi đội tạm dừng, không có allow nào mà người không bấm):
- Hub chỉ đặt `countdown_ms = Some(..)` khi policy trả `AskUser{countdown}` (T10 mức 5), yêu cầu `reversible` và
  đội **không** tạm dừng.
- Hub chấp nhận `Decide{decision: Allow, origin: Countdown}` chỉ khi cả ba đúng: yêu cầu đó đang có
  `countdown_ms = Some(c)`, đội không tạm dừng, và `now ≥ created_at + c`. Sai bất kỳ điều kiện nào: **giữ nguyên**
  yêu cầu trong hàng đợi (không deny), gửi `Feed` "Cần trưởng nhóm bấm duyệt". `origin: Countdown` kèm `Deny` cũng
  bị từ chối như vậy (đếm ngược chỉ được allow).
- Allow được chấp nhận ghi `decided_by = countdown`; người bấm ghi `decided_by = user`.
- `TeamPause`: hub đặt `countdown_ms = None` cho **mọi** yêu cầu đang chờ rồi broadcast `TeamUpdated`. `TeamResume`
  không khôi phục đếm ngược cho yêu cầu cũ; chỉ yêu cầu tới sau mới có thể có đếm ngược.

### 3.5 SQLite (`hub-core/src/store.rs`)

Tệp: `<data_dir>/arcturus/arcturus.db` (`dirs::data_dir()`), `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`.
Migration đánh số bằng `PRAGMA user_version`.

```sql
-- v1
CREATE TABLE events (
  id TEXT PRIMARY KEY, ts TEXT NOT NULL, session_id TEXT NOT NULL,
  kind TEXT NOT NULL, event_json TEXT NOT NULL, raw_json TEXT
);
CREATE INDEX events_ts ON events(ts);
CREATE INDEX events_session ON events(session_id, ts);

CREATE TABLE approvals (
  request_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, agent_id TEXT NOT NULL,
  tool TEXT NOT NULL, command_original TEXT NOT NULL, command_normalized TEXT NOT NULL,
  risk TEXT NOT NULL, group_key TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending','allowed','denied','timed_out','cancelled','handed_off')),
  decided_by TEXT, created_at TEXT NOT NULL, decided_at TEXT, latency_ms INTEGER, channel TEXT
);
CREATE INDEX approvals_created ON approvals(created_at);

-- v2 (T9, T10)
CREATE TABLE batch_rules (group_key TEXT PRIMARY KEY, opened_at TEXT NOT NULL, expires_at TEXT NOT NULL);
CREATE TABLE trust_grants (
  id TEXT PRIMARY KEY, subject TEXT NOT NULL, pattern TEXT NOT NULL, level INTEGER NOT NULL,
  granted_at TEXT NOT NULL, expires_at TEXT NOT NULL, reason TEXT NOT NULL, revoked_at TEXT
);
CREATE TABLE streaks (
  subject TEXT NOT NULL, pattern TEXT NOT NULL, approved_in_row INTEGER NOT NULL DEFAULT 0,
  first_at TEXT, last_at TEXT, last_incident_at TEXT, PRIMARY KEY (subject, pattern)
);
CREATE TABLE trust_log (id TEXT PRIMARY KEY, ts TEXT NOT NULL, subject TEXT, pattern TEXT,
  from_level INTEGER, to_level INTEGER, reason TEXT NOT NULL);
```

Ghi qua một task riêng (`tokio::task::spawn_blocking` hoặc thread riêng) nhận lệnh từ `mpsc::channel(1024)`.
Kênh đầy: bỏ bản ghi `events` (đếm số bị bỏ, log warn), **không bao giờ** bỏ bản ghi `approvals`.
Dọn TTL: mỗi 6 giờ xóa `events` cũ hơn `event_ttl_days`.

### 3.6 Trạng thái agent (`hub-core/src/state.rs`)

```rust
pub struct TeamState {
    pub agents: BTreeMap<AgentId, AgentRuntime>,
    pub focus: bool,                 // cờ tạm dừng nằm trong Approvals (D3, T5)
}
pub struct AgentRuntime { pub view: AgentView, pub done_until: Option<OffsetDateTime> }

impl TeamState {
    /// Áp một Event, trả danh sách thay đổi để broadcast.
    pub fn apply(&mut self, ev: &Event, personas: &PersonaBook, now: OffsetDateTime) -> Vec<ServerMsg>;
    /// Gọi định kỳ (mỗi 1 s chỉ khi có agent trong trạng thái Done) để chuyển Done → Idle.
    pub fn tick(&mut self, now: OffsetDateTime) -> Vec<ServerMsg>;
}
```

Bảng chuyển trạng thái (PreToolUse theo tool):

| Tool | activity | mood | act_text |
|---|---|---|---|
| Edit, Write, MultiEdit, NotebookEdit | typing | working | "Đang sửa <tên file>" |
| Read, Grep, Glob, LS | reading | working | "Đang đọc <file/pattern>" |
| Bash | running | working | "Đang chạy <lệnh, 60 ký tự>" |
| WebFetch, WebSearch | reading | working | "Đang tra cứu" |
| Task/Agent | thinking | working | "Đang giao việc cho trợ lý" |
| khác | thinking | working | "Đang dùng <tool>" |

Sự kiện khác: PermissionRequest → waiting/approval; PostToolUseFailure, StopFailure → mood error (giữ tới sự kiện
kế tiếp); Stop → done trong 3 giây rồi idle; SessionEnd → xóa agent (và agent con), hủy yêu cầu đang chờ.

`tick` chỉ được lên lịch khi có ít nhất một agent ở trạng thái Done (không timer thường trực).

### 3.7 Persona và cấu hình (`persona.rs`, `config.rs`)

```toml
[hub]
bind = "127.0.0.1:7878"
approval_timeout_secs = 110
reconcile_interval_secs = 30
event_ttl_days = 30
handoff_max_bytes = 4096       # lệnh dài hơn (byte UTF-8) hoặc nhiều dòng hơn: trả về terminal (§3.4, T5b); 1–1048576
handoff_max_lines = 50
held_input_max_bytes = 8388608 # tổng byte tool_input đang giữ; vượt thì yêu cầu mới nhận {} (T5c · ARC-43); 65536–268435456
min_review_ms = 1000           # allow do người bấm sớm hơn chừng này sau khi thẻ hiện thì bị giữ (T5d · ARC-44); 0–10000, 0 = tắt

[team]
main_roster_max = 5

[trust]
streak_threshold = 20
streak_window_days = 7
grant_days = 30

[[persona]]
match_cwd = "**/.claude/worktrees/hub*"
name = "Khoa"
role = "backend"
palette = { hair = "#1F1F24", outfit = "#4C8DF6", acc = "#9AA3B5" }
```

- Thiếu file: dùng mặc định. File hỏng: hub không khởi động, báo dòng lỗi.
- Không khớp persona: tên lấy lần lượt từ danh sách sao chòm Boötes chưa dùng:
  `["Muphrid","Seginus","Nekkar","Xuange","Genghe","Merga","Qigong","Ceginus","Alkalurops","Alazal"]`, hết thì `"Agent <4 ký tự đầu session_id>"`.
  Bảng màu mặc định chọn theo hash ổn định (FNV-1a) của session_id từ 6 bảng màu cố định.
- Tên đã gán cho một session_id được giữ nguyên trong suốt phiên.
- Khớp `match_cwd`: trước khi so, đổi mọi `\` thành `/` (đường dẫn Windows); trên Windows so không phân biệt hoa
  thường. Persona khớp nếu `cwd` **hoặc một thư mục cha của `cwd`** khớp glob (agent chạy trong thư mục con của
  worktree vẫn đúng persona). Nhiều persona cùng khớp: lấy persona khai báo trước trong file.

## 4. Kiến trúc nội bộ hub

```rust
pub struct Hub {
    pub cfg: Config,
    pub host: String,
    pub personas: PersonaBook,
    pub state: tokio::sync::RwLock<TeamState>,
    pub pending: tokio::sync::Mutex<HashMap<RequestId, Pending>>,
    pub tx: tokio::sync::broadcast::Sender<ServerMsg>,   // capacity 256
    pub store: StoreHandle,                               // mpsc tới writer
    pub policy: tokio::sync::Mutex<Policy>,
    pub events_in: tokio::sync::mpsc::Sender<HookInput>,  // từ /hooks/event
}

pub struct Pending {
    pub view: ApprovalView,
    pub reply: tokio::sync::oneshot::Sender<Decided>,
}
pub struct Decided { pub decision: Decision, pub by: DecidedBy, pub message: Option<String> }
```

- Một task nền đọc `events_in`, gọi `hook::to_event`, `state.apply`, ghi store, broadcast. Thứ tự sự kiện của cùng
  session được giữ nguyên.
- Không giữ khóa `state` qua `.await` dài. Không bao giờ giữ đồng thời `state` và `pending` (tránh deadlock;
  nếu cần cả hai thì lấy `pending` trước).
- Broadcast lag: client chậm nhận `RecvError::Lagged` → server gửi lại `Snapshot` cho client đó.
- Tắt hub (SIGINT/SIGTERM): trả `{}` cho mọi yêu cầu đang chờ (để Claude Code hỏi terminal), flush store, thoát.

## 5. Quy ước test

- Test đơn vị trong cùng file (`#[cfg(test)]`), test tích hợp ở `crates/hub/tests/`.
- Test tích hợp dựng hub thật trên cổng ngẫu nhiên (`127.0.0.1:0`) với thư mục dữ liệu tạm.
- Thời gian: hàm nhận `now` làm tham số; timeout dùng `tokio::time::pause()` + `advance()` trong test.
- Fixture: `fixtures/hooks/<EventName>/<mô-tả>.json`; test parse chạy trên **toàn bộ** thư mục (glob).
- UI: Vitest + Testing Library; giả lập WebSocket bằng lớp `FakeSocket` trong `ui/src/test/`.
- Tên test mô tả hành vi bằng tiếng Anh snake_case, ví dụ `timeout_returns_empty_object_never_allow`.

---

## 6. Từng task

### T0 · ARC-21 · Scaffold

**Tạo**
- `Cargo.toml` workspace; crate `hub-core` (lib), `hub` (bin), `tools/replay` (bin), mỗi crate một test mẫu.
- `crates/app`: `pnpm create tauri-app` hoặc khởi tạo thủ công Tauri 2, frontend trỏ tới `../../ui` (dev URL Vite, build `ui/dist`).
- `ui/`: Vite React TS, `tsconfig` strict, Vitest với jsdom, một test mẫu.
- `.github/workflows/ci.yml`: Ubuntu; cài Rust stable, pnpm; chạy `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -C ui install --frozen-lockfile`, `pnpm -C ui test --run`, `pnpm -C ui build`. Bỏ qua build `crates/app` trên CI ở v0 nếu thiếu thư viện hệ thống (ghi chú lý do).
- `rust-toolchain.toml` (channel stable), `rustfmt.toml` (mặc định), `clippy.toml` không cần.

**Kiểm tra:** mọi lệnh trong mục "Lệnh" của `CLAUDE.md`.

### T1 · ARC-22 · Hook lab

**Tạo**
- `hub` có chế độ `--record <dir>`: endpoint `/hooks/event` và `/hooks/permission` chỉ ghi body thô ra
  `<dir>/<hook_event_name>/<ulid>.json` và trả `204` / `{}` (không quyết định gì). Vẫn bắt buộc token.
- `tools/scrub.py` hoặc lệnh `replay scrub`: thay đường dẫn home bằng `/home/user`, xóa token, rút ngắn transcript_path.
- Settings thí nghiệm trong `lab/settings.local.json` (không đụng `~/.claude/settings.json`), dùng cấu hình như
  `hooks/arcturus-dogfood.json` nhưng trỏ cả `PermissionRequest` tới endpoint ghi băng.

**Làm**
- Dùng agent `hook-spike` cho từng câu hỏi Q1–Q4 trong `docs/HOOK_FINDINGS.md`.
- Q1: cho endpoint `/hooks/permission` ngủ 20 giây trước khi trả `{}` (cờ `--record-delay 20`), quan sát terminal.

**Xong khi:** ≥30 fixture đúng thư mục; bảng Q1–Q4 có kết quả và fixture; cập nhật mục "Kiến thức đã kiểm chứng"
trong `CLAUDE.md` nếu có điều mới (đề xuất, chờ duyệt).

### T2 · ARC-23 · Schema và chuẩn hóa

**Tạo:** `schema.rs` (3.1), `hook.rs` (3.2), các test.

**Test bắt buộc**
- `every_fixture_parses_to_event` (glob toàn bộ `fixtures/hooks`)
- `unknown_event_name_maps_to_unknown_kind`
- `missing_session_id_uses_unknown`
- `tool_summary_bash_uses_command`, `tool_summary_edit_uses_file_path`
- Snapshot `insta::assert_json_snapshot!` cho mỗi loại sự kiện (một fixture đại diện)

### T3 · ARC-24 · Luồng duyệt

**Tạo:** `hub/src/approvals.rs`, `hub/src/hooks.rs` (phần permission), test tích hợp `tests/permission.rs`.

**Thuật toán `/hooks/permission`**
```
1. auth (401 nếu sai)
2. input = HookInput(body); ev = to_event(input)   // EventKind::ApprovalRequested
3. state.apply(ev)  -> agent chuyển waiting/approval ; broadcast
4. ctx = ApprovalCtx { agent, tool, tool_input, cwd }
5. outcome = policy.decide(&ctx)                    // T3: Policy giả luôn trả AskUser{risk: Medium};
                                                    // test tiêm được Policy giả trả AutoAllow (trait hoặc hàm dựng cho test)
   nếu team.paused và outcome là AutoAllow: outcome = AskUser{risk đã tính}
   // tạm dừng tắt mọi AutoAllow (T14), phải chạy TRƯỚC khi xét nhánh dưới; tạm dừng không tự từ chối
6. match outcome:
   - AutoAllow{reason}  -> ghi approvals(status=allowed, decided_by=policy) VÀ CHỜ writer xác nhận đã commit
                           (oneshot ack, tối đa 1 s) -> trả allow
                           ghi lỗi / kênh đóng / quá 1 s -> log error, outcome = AskUser{risk đã tính}, tiếp bước 7
                           // không có bản ghi kiểm toán thì không tự allow (quy tắc bất biến 1)
   - AskUser{risk,…}    -> tiếp bước 7
   (Policy KHÔNG có nhánh AutoDeny trong v0.)
7. tạo RequestId, ApprovalView, (tx, rx) = oneshot; pending.insert; store approvals(pending); broadcast ApprovalRequested
8. match timeout(cfg.approval_timeout, rx):
     Ok(Ok(Decided{Allow,..})) -> trả allow
     Ok(Ok(Decided{Deny,..}))  -> trả deny (+message)
     Ok(Err(_)) /* sender bị drop: phiên kết thúc, hủy */ -> trả {}
     Err(_) /* hết giờ */ -> pending.remove; store timed_out; broadcast resolved(timeout) -> trả {}
9. sau khi trả: agent về activity thinking (sẽ được sự kiện kế tiếp cập nhật)
```
- Hủy khi `SessionEnded`: xóa mọi pending của session (drop `reply` sender) → handler rơi vào nhánh `Ok(Err)` và trả `{}`; ghi `status=cancelled`.
- `ClientMsg::Decide`: lấy `pending.remove(id)`; không tồn tại (đã hết giờ) → gửi `Feed` "Yêu cầu đã hết hạn", không lỗi.
  `origin: Countdown` kiểm theo quy tắc "Đếm ngược phủ quyết và tạm dừng" (§3.4) **trước** khi remove; không đạt thì
  giữ nguyên pending. Ở T3 chưa có đếm ngược (`countdown_ms` luôn `None`) nên mọi `origin: Countdown` đều bị từ chối.
- Một handler không bao giờ được trả `allow` ở nhánh lỗi: viết hàm `fn response_for(result: Result<…>) -> Json<Value>` với `match` đầy đủ và test cho từng nhánh.

**Test bắt buộc** (`tests/permission.rs`)
- `user_allow_returns_allow_decision`
- `user_deny_returns_deny_decision`
- `timeout_returns_empty_object_never_allow` (tokio time pause)
- `session_end_while_pending_returns_empty_object`
- `two_sessions_concurrent_requests_routed_correctly`
- `wrong_token_returns_401`
- `malformed_json_returns_400`
- `shutdown_returns_empty_object_for_pending`
- `decide_on_expired_request_is_noop`
- `paused_converts_auto_allow_to_ask_user` (Policy giả trả AutoAllow, đặt `paused = true`)
- `auto_allow_store_failure_falls_back_to_ask_user` (writer giả trả lỗi hoặc không ack)
- `countdown_origin_rejected_without_countdown` (yêu cầu vẫn pending, không bị deny)
- `countdown_origin_rejected_before_elapsed`, `countdown_origin_rejected_while_paused`
- `countdown_allow_records_decided_by_countdown`

**Đã làm ở T3 (lệch so với trên, đã duyệt kế hoạch)**
- Hủy khi đã được trả lời ở terminal (Q1b: Claude Code **không** đóng kết nối, `docs/HOOK_FINDINGS.md`): sự kiện kế
  tiếp của cùng agent (`PostToolUse`/`PostToolUseFailure` cùng `tool_name` + `tool_input`, `Stop`, `StopFailure`,
  `UserPromptSubmit`, `SubagentStop`) hủy pending → `{}`. `Notification` không hủy. Client đóng kết nối (tiến trình thoát)
  → guard trong handler hủy, ghi `cancelled`.
- Kiểu tạm trong `hub/src/approvals.rs`, thay ở task sau: trait `Policy` + `AlwaysAsk` (T8 chuyển sang
  `hub-core/src/policy/`), trait `ApprovalStore` + `NoStore` (T6; không có store thì AutoAllow luôn rơi về hỏi người),
  `ApprovalNotice` (T5 đổi thành `ServerMsg`, dựng `ApprovalView`), `Approvals::decide` trả `DecideResult` (T5 đổi
  `Expired`/`Held` thành `Feed`). Bước 3 (`state.apply`) chưa gọi: T4. Cờ tạm dừng nằm trong `Approvals`
  (`set_paused`), T5 nối với `TeamPause`/`TeamResume`.
- Hàng đợi pending dùng `std::sync::Mutex` (không giữ qua `.await`) để guard hủy trong `Drop` khóa được.
- `--approval-timeout` (1–115 s, mặc định 110) thay cho `approval_timeout_secs` tới khi có `config.rs` (T4).
- Theo security audit T3: giới hạn 64 yêu cầu đang giữ toàn hub, 16 mỗi phiên (vượt → `{}` ngay); lượt AutoAllow có
  `request_id` riêng, rơi về hỏi người thì ghi `cancelled` cho id đó và tạo id mới; log chuỗi từ agent bằng Debug.
- Chấp nhận ở v0: ai có token đều gửi được `SessionEnd`/`Stop` giả để xóa thẻ duyệt của phiên khác. Kết quả chỉ là `{}`
  (hộp thoại terminal vẫn còn, Q1/Q2), không bao giờ là allow/deny.

### T4 · ARC-25 · State machine và persona

**Tạo:** `state.rs` (3.6), `persona.rs`, `config.rs` (3.7).

**Test bắt buộc**
- Mỗi dòng bảng 3.6 một test (`pre_tool_edit_sets_typing` …)
- `stop_sets_done_then_idle_after_3s`
- `subagent_start_creates_child_with_parent`, `subagent_stop_removes_child`
- `session_end_removes_agent_and_children`
- `persona_matches_by_cwd_glob`, `unmatched_gets_star_name_in_order`, `name_stable_for_session`
- `persona_matches_windows_backslash_path`, `persona_matches_subdirectory_of_worktree`
- `palette_hash_is_deterministic`
- `invalid_config_fails_with_line_number`

**Đã làm ở T4 (lệch so với trên, đã duyệt kế hoạch)**
- §3.1: thêm `Event.cwd` và `AgentRef.agent_type` (cả hai `Option`, `#[serde(default)]`, không tăng `v`): `apply` cần
  `cwd` để khớp persona và hiển thị, subagent dùng `agent_type` làm vai trò. 11 snapshot T2 cập nhật (chỉ thêm 2 trường).
- `protocol.rs` tạo ở T4 với phần T4 cần: `ServerMsg::{AgentUpdated, AgentRemoved}`, `AgentView`, `Palette` đúng §3.4.
  T5 thêm các biến thể còn lại, `ClientMsg`, `ApprovalView`, `TeamView`.
- `act_text` ngoài bảng §3.6: SessionStart và sau `done` → "Đang rảnh"; UserPromptSubmit, PostToolUse, SubagentStart →
  "Đang suy nghĩ"; PermissionRequest → "Chờ duyệt: <tool>"; Stop → "Xong"; PostToolUseFailure, StopFailure → "Gặp lỗi"
  (giữ activity). Ký tự điều khiển trong `act_text` đổi thành khoảng trắng. Read → tên file; Grep/Glob/LS → pattern
  (≤60 ký tự); LS không có trường tóm tắt → "Đang đọc thư mục".
- Chỉ gửi `AgentUpdated` khi có thay đổi nhìn thấy được; `last_event_at` vẫn cập nhật. `TeamState::next_wakeup()` cho
  hub biết lúc cần gọi `tick` (không timer khi không có agent `done`). Notification/Unknown không xóa mood error, không
  hủy đếm `done`.
- Agent tạo từ sự kiện đầu tiên của phiên lạ (F1); sự kiện có `agent_id` lạ tạo con, và tạo cả cha (trạng thái
  "Đang giao việc cho trợ lý") nếu chưa có.
- Persona: subagent không lấy persona (tránh trùng tên cha), nhận tên sao riêng, vai trò `agent_type` (thiếu → "trợ lý"),
  màu theo hash `agent.id`. Phiên không khớp: vai trò "thành viên". Tên sao nhả lại khi agent bị xóa.
- `config.rs`: khóa lạ là lỗi (có số dòng); `approval_timeout_secs` 1–115 (khớp `--approval-timeout` T3); `bind` phải
  loopback; màu persona chỉ nhận `#RRGGBB` (được chèn vào SVG); glob hỏng báo `persona[i].match_cwd`.
- Chưa nối vào `crates/hub` (đợi PR T3 merge): gọi `state.apply` trong vòng sự kiện và bước 3 của `/hooks/permission`,
  thay `--approval-timeout` bằng `Config.hub.approval_timeout_secs`, hẹn `tick` theo `next_wakeup`, đồng bộ
  `TeamState.paused` với `Approvals::set_paused` (T5).

### T5 · ARC-26 · WebSocket

**Tạo:** `protocol.rs` (3.4), `hub/src/ws.rs`, `docs/PROTOCOL.md` (chép định nghĩa 3.4 + ví dụ JSON cho mỗi loại).

**Hành vi**
- Kết nối: kiểm token → gửi `Snapshot` → vòng lặp `select!` giữa `broadcast::Receiver` và tin nhắn client.
- Ping mỗi 30 giây; client không trả lời 2 lần → đóng.
- Thông điệp client không hợp lệ → gửi `Feed` báo lỗi, không đóng kết nối.
- `TeamPause` / `TeamResume`: đặt `TeamState.paused`, broadcast `TeamUpdated`. `TeamPause` xóa `countdown_ms` của mọi
  yêu cầu đang chờ (§3.4); `TeamResume` không khôi phục. Phần hub của "Tạm dừng cả đội" làm ở
  task này (làn `hub`); T14 chỉ làm nút và hiển thị (làn `ui`).
- `GET /approvals/{request_id}/input` (§3.3): trả `tool_input` đầy đủ từ `pending`; không còn chờ → `404`.

**Test bắt buộc**
- `ws_rejects_without_token`, `ws_sends_snapshot_first`
- `ws_team_pause_sets_paused_and_broadcasts`, `ws_team_resume_clears_paused`
- `team_pause_clears_countdown_of_pending`, `team_resume_does_not_restore_countdown`
- `approval_input_returns_full_tool_input`, `approval_input_unknown_returns_404`, `approval_input_requires_token`
- `ws_decide_resolves_pending_request` (kết hợp với T3)
- `ws_lagged_client_gets_fresh_snapshot`
- `protocol_roundtrip_serde` cho mọi biến thể

**Quyết định đã duyệt (T5, chia hai PR: T5a rồi T5b)**
- D1: token UI riêng cho `/ws` và `/approvals/*`, hub tự sinh mỗi lần chạy, không nằm trong môi trường Claude Code
  (Bash tool thừa hưởng `ARCTURUS_TOKEN`, `docs/HOOK_FINDINGS.md` F6). Làm ở T5b.
- D2: `ApprovalResolved.decision: Option<Decision>`, `null` khi `decided_by` là `timeout`/`cancelled`.
- D3: bỏ `TeamState.paused`; cờ duy nhất nằm trong `Approvals` (kiểm đếm ngược dưới cùng khóa). `TeamView.paused` đọc
  từ đó.

**Đã làm ở T5a** (nối T3 + T4 vào `crates/hub`, chưa có WebSocket)
- `hub/src/hub.rs`: `Hub` giữ `Config`, `PersonaBook`, `TeamState`, `Approvals` và kênh broadcast `ServerMsg` (256).
  Vòng sự kiện: `to_event` → `approvals.on_event` → `state.apply` → broadcast (luồng duyệt trước, nên `SessionEnd` gửi
  `ApprovalResolved` trước `AgentRemoved`). `/hooks/permission`: `state.apply` (bước 3, nhân vật chờ duyệt) →
  `approvals.request` → khi có quyết định (người hoặc AutoAllow) `TeamState::approval_finished` đưa nhân vật về suy nghĩ
  (bước 9). Hết giờ/hủy/đầy giữ "chờ duyệt" vì terminal vẫn đang hỏi.
- Task tick: ngủ tới `next_wakeup()` (watch channel), không hẹn giờ khi không có agent `done`.
- `main.rs` đọc `arcturus.toml` (`--config`, mặc định `./arcturus.toml`); `--approval-timeout` bị bỏ, timeout và bind lấy
  từ cấu hình (`--bind` vẫn ghi đè được).
- `ApprovalNotice` của T3 bị thay bằng `ServerMsg::ApprovalRequested{ApprovalView}` / `ApprovalResolved`. Thẻ duyệt
  broadcast đã cắt nội dung (`hub/src/view.rs`); `tool_input` đầy đủ chỉ nằm trong hàng đợi (`Approvals::tool_input`).
- Trước T8: `command_normalized` hiện ký tự điều khiển/zero-width/bidi thành `⟨U+XXXX⟩` và gắn `hidden_chars`;
  `danger_spans = []`, `reversible = false`, `group_key = tool`. T8 thay bằng normalize đầy đủ.
- `protocol.rs` thêm `ApprovalView`, `Suspicion`, `TeamView`, `DecideOrigin` (chuyển từ `hub`), `ApprovalRequested`,
  `ApprovalResolved`, `TeamUpdated`. `Hub::set_paused` / `set_focus` broadcast `TeamUpdated` (T5b nối với `ClientMsg`).
- Theo security audit T5a: danh sách ký tự ẩn gồm Cc, toàn bộ Default_Ignorable_Code_Point, Cf khác, Zl/Zp, Co, khoảng
  trắng không phải ASCII và U+2800; chuỗi lặp hiện `⟨U+XXXX×N⟩`; cờ `hidden_chars` xét mọi chuỗi/khóa của `tool_input`
  gốc. Chỉ Edit/Write/MultiEdit/NotebookEdit bị cắt, MultiEdit cắt từng `old_string`/`new_string` (giữ mảng,
  `replace_all`). Thẻ duyệt dựng một lần, cache trong hàng đợi; `ApprovalRequested` và bản ghi `pending` gửi khi còn giữ
  khóa (không còn thẻ "ma" tới sau `ApprovalResolved`). `TeamState` tối đa 256 agent, `cwd` tối đa 4.096 ký tự. Hub log
  đường dẫn tuyệt đối của file cấu hình.
- Còn mở (cần quyết hoặc làm ở task sau): giới hạn hiển thị lệnh Bash rất dài (bất biến 3 hiện cấm cắt Bash; body tối
  đa 1 MB) — cần người quyết; T5b: serialize mỗi thông điệp broadcast một lần, giới hạn hàng gửi mỗi client, UI bắt mở
  bản đầy đủ khi `truncated`; T8: bảng Unicode đầy đủ (gồm Cn); cấu hình mặc định nên ở thư mục cấu hình người dùng
  trước khi T8/T10 đưa policy vào file.

**Đã làm ở T5b** (đã duyệt kế hoạch: L1–L5, P1–P3)
- Lệnh quá dài trả về terminal (§3.4 "Lệnh quá dài"): quyết định của người duyệt cho mục còn mở ở trên; quy tắc bất
  biến 3 trong `CLAUDE.md` thêm câu "Hub không bao giờ cắt bớt một lệnh mà nó cho người duyệt…". L1: Edit/Write/
  MultiEdit/NotebookEdit vẫn cắt nội dung (không trả về terminal). L2: `approvals.status = 'handed_off'` (§3.5). L3: kiểu
  riêng `HandoffView`, id `hof_`. L4: ngưỡng 4.096 byte / 50 dòng, cấu hình 1–1.048.576.
- `/ws` (`hub/src/ws.rs`): Snapshot trước rồi delta; mỗi `ServerMsg` serialize một lần thành frame dùng chung; client
  tụt nhận Snapshot mới; client không nhận frame trong 10 s bị ngắt; ping 30 s, lỡ 2 lần thì đóng; frame tối đa 64 KB;
  `Decide` tối đa 20/s mỗi kết nối, lý do tối đa 500 ký tự, mọi quyết định được log kèm kết nối; Ctrl-C gửi Close.
  `Feed` trả lời lệnh (hết hạn, bị giữ, duyệt-trong-terminal, chưa hỗ trợ, thông điệp sai) chỉ gửi cho client đó.
- Token UI riêng (D1) và kiểm `Host`/`Origin` (§3.3). P2: Windows dựa vào quyền mặc định của `%APPDATA%`. P3: cấu hình
  vẫn đọc từ thư mục hiện tại tới T8.
- `GET /approvals/{id}/input` (§3.3). `docs/PROTOCOL.md` mô tả đầy đủ giao thức cho làn `ui`.
- Theo security audit T5b (không có mức cao): `ApprovalView`/`HandoffView` thêm `tool_options` (Bash: trường ngoài
  `command`/`description`, ví dụ `dangerouslyDisableSandbox`, hiện tách khỏi lệnh); allow thẻ `truncated` bị giữ
  (`HeldReason::NotReviewed`) tới khi `GET /approvals/{id}/input` đã được gọi; tối đa 8 kết nối WebSocket, lệnh đội chung
  giới hạn 20/s với `Decide`; đổi cờ đội và broadcast tuần tự (không còn `TeamUpdated` cũ); file tạm của token UI luôn
  tạo mới (không theo symlink, không giữ quyền cũ), thư mục 0700 trên Unix; origin Vite dev chỉ ở bản debug.
- Còn mở sau audit T5b: (1) token UI vẫn đọc được bởi agent đã có quyền chạy mã dưới cùng người dùng (giới hạn của D1) —
  cần người quyết giữa sàn thời gian phản ứng cho allow, hoặc app khởi động hub và trao token qua pipe; đề xuất ticket T8:
  luật danger + cờ khi `tool_input` chạm `arcturus/ui-token` hoặc `/ws`. (2) Edit/MultiEdit rất lớn vẫn là thẻ (L1), có
  thể tới gần 1 MB mỗi thẻ trong Snapshot; cân nhắc ngưỡng tổng kích thước cho nhóm này.

### T6 · ARC-27 · Lưu trữ

**Tạo:** `store.rs` (3.5) với `StoreHandle` (mpsc) và `StoreReader` (kết nối đọc riêng).

**Test bắt buộc**
- `migrations_apply_from_empty_and_are_idempotent`
- `approval_lifecycle_persisted` (pending → allowed, latency_ms đúng)
- `events_older_than_ttl_are_deleted`
- `full_channel_drops_events_never_approvals`
- `survives_restart` (đóng, mở lại, dữ liệu còn)

**Đã làm ở T6** (lệch so với trên, cần người duyệt khi review PR)
- `hub_core::store`: `Store` (chủ thread ghi, `close` ghi hết rồi dừng), `StoreHandle` (clone được, không bao giờ chặn),
  `StoreReader` (kết nối chỉ đọc, mở khi cần), `EventRing`. Một kênh duy nhất giữ thứ tự ghi (`allowed` rồi `cancelled` của
  cùng id); thread ghi gom tới 256 lệnh mỗi transaction; lượt lỗi thì thử từng lệnh riêng.
- Kênh không giới hạn cứng cho `approvals` (đã bị chặn bởi hàng đợi duyệt: tối đa 64 đang giữ); sự kiện bị bỏ khi đã có
  1.024 sự kiện **hoặc** 4 MiB đang chờ (đếm số bị bỏ, warn lúc bắt đầu bỏ, info lúc thông lại).
- `ApprovalStore` (trait của hub) thêm `record_event`; `ApprovalRecord`/`ApprovalStatus` chuyển sang `hub_core::store`;
  `ApprovalRecord.detail` (bản ghi đầu của một id) mang phần thẻ người duyệt đã thấy: `command_original`/`command_normalized`
  của `ApprovalView` (nội dung Edit/Write đã cắt), với lệnh trả về terminal là đầu + cuối lệnh. Xác nhận cho AutoAllow
  gửi sau commit.
- `latency_ms` = `decided_at − created_at` (tính ở thread ghi từ `created_at` đã lưu); `channel`: `ws` (người, đếm
  ngược), `policy`, NULL (hết giờ, hủy). `risk` của dòng `handed_off` là đánh giá thật của lệnh (hub vẫn không quyết định).
- Thời gian lưu UTC `YYYY-MM-DDTHH:MM:SS.mmmZ` (độ dài cố định: so chuỗi đúng thứ tự). `raw_json` chỉ lưu khi JSON hook
  gốc ≤ 256 KiB (đo trước không cấp phát), lớn hơn thì NULL; `event_json` luôn có.
- `PRAGMA cache_size=-512` mỗi kết nối, `busy_timeout` 5 s; Unix: thư mục 0700, file 0600 (nhật ký có lệnh, đường dẫn).
  Cơ sở dữ liệu phiên bản mới hơn hub (`user_version` > 1) → hub không khởi động.
- Dọn TTL lúc mở và mỗi 6 giờ trong thread ghi (`recv_timeout`, không timer khác). `hub --db <file>`, mặc định
  `<data_dir>/arcturus/arcturus.db`; mở lỗi thì hub không khởi động. Tắt hub: hủy yêu cầu đang chờ rồi `close` (bản ghi
  `cancelled` được ghi trước khi thoát).
- Ring buffer (ARC-27): `EventRing` 2.000 sự kiện **và** 2 MiB (sự kiện có `cwd`, id do agent gửi không bị chặn độ dài),
  `Hub::recent_events(n)`; chưa có ai đọc (T19).
- Đọc theo phiên sắp `ts, rowid` (cùng mili giây: thứ tự ghi).

### T7 · ARC-28 · Đối soát

**Tạo:** `hub/src/reconcile.rs`.
- Mỗi `reconcile_interval_secs`: `tokio::process::Command::new("claude").args(["agents","--json"])`, timeout 5 giây.
- Lệnh lỗi hoặc không có `claude`: log debug, bỏ qua lần này (không xóa gì).
- Phiên có trong state nhưng không có trong kết quả **hai lần liên tiếp** → coi như SessionEnded.
- Trường JSON chốt ở Q8; parse chịu lỗi (thiếu trường thì bỏ qua phần tử).

**Test:** dùng trait `AgentLister` để giả lập: `removes_session_missing_twice`, `keeps_session_missing_once`,
`command_failure_removes_nothing`.

**Đã làm ở T7**
- `hub/src/reconcile.rs`: trait `AgentLister` (`impl Future` thay cho `async fn` trong trait để có `Send`), `ClaudeAgents`
  (`claude agents --json`, `kill_on_drop`, timeout 5 giây, đầu ra ≤1 MiB, không cửa sổ console trên Windows), `parse_agents`
  (chịu lỗi: phần tử thiếu/sai kiểu `sessionId` bị bỏ qua, không phải mảng thì không kết luận), `Reconciler` (bộ đếm thuần),
  `tick` và `spawn` (mỗi `reconcile_interval_secs`, giữ `Weak<Hub>` nên dừng khi hub bị drop). `main` khởi động task.
- **Không polling thường trực**: nếu hub không giữ phiên nào thì không chạy `claude` (quy tắc bất biến 6); mỗi lần chạy ≈270 ms
  ở một tiến trình riêng, không tính vào RSS của hub.
- **Lệch/làm chặt so với đặc tả**: (1) "vắng hai lần liên tiếp" tính trên hai lần quan sát **thành công**: lệnh lỗi hay hết giờ
  xóa bộ đếm (đặc tả chỉ nói bỏ qua lần đó); (2) chỉ dọn phiên đã **im lặng ≥ 2 chu kỳ** (không có sự kiện hook), vì phiên chạy
  một lệnh dài không bắn sự kiện, và chưa biết (Q8) phiên `-p`/nền có được liệt kê không; dọn nhầm là fail-safe (yêu cầu đang
  giữ trả `{}`, agent hiện lại ở sự kiện kế tiếp) nhưng gây nhiễu.
- Dọn bằng cách đưa một `SessionEnd` giả (`reason: "reconcile"`) vào `Hub::ingest`: cùng đường với `SessionEnd` thật (hủy yêu cầu
  đang chờ → `ApprovalResolved(cancelled)` trước `AgentRemoved`, xóa agent con, ghi sự kiện).
- `TeamState::sessions()`: phiên suy từ id agent gốc; thời điểm sự kiện gần nhất là max của phiên và mọi agent con.
- Test: 7 đơn vị (`reconcile.rs`: đếm, đứt chuỗi khi lỗi, hoạt động gần đây, parse chịu lỗi, thiếu chương trình) và 6 tích hợp
  (`hub/tests/reconcile.rs`: dọn như SessionEnd thật gồm hủy yêu cầu và thứ tự thông điệp, lỗi không xóa gì, agent con, hoạt
  động gần đây, hub rỗng không chạy lister, task theo chu kỳ và dừng khi hub drop). Có thêm test `#[ignore]` chạy `claude` thật.

### T8 · ARC-29 · Danger list, chuẩn hóa, rủi ro

**`normalize.rs`**
```rust
pub struct Normalized { pub text: String, pub segments: Vec<String>, pub suspicious: Vec<Suspicion> }
pub fn normalize(cmd: &str) -> Normalized;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Suspicion { HiddenChars, Subshell, Eval, Encoded }   // gửi tới UI trong ApprovalView.suspicions
```
1. Bỏ ký tự vô hình và điều khiển hướng: U+200B–U+200F, U+202A–U+202E, U+2060–U+2064, U+2066–U+2069, U+FEFF, U+00AD;
   ghi `Suspicion::HiddenChars` nếu có.
2. Thay mọi khoảng trắng Unicode (U+00A0, U+2000–U+200A, U+3000, tab) bằng dấu cách; gộp nhiều dấu cách.
3. Tách `segments` theo `;`, `&&`, `||`, `|`, xuống dòng (không tách bên trong dấu nháy đơn/kép).
4. Đánh dấu `Suspicion::Subshell` nếu có `$(`, dấu backtick; `Suspicion::Eval` nếu segment bắt đầu bằng `eval`, `source`, `.`;
   `Suspicion::Encoded` nếu có `base64 -d`/`--decode`, `xxd -r`, `printf '\x`.

**`danger.rs`**: danh sách luật, mỗi luật có `id`, `description`, hàm khớp trên `Normalized` hoặc trên tool khác Bash.
| id | Khớp |
|---|---|
| `rm_recursive_force` | segment có lệnh `rm` với cờ chứa cả `r`/`R` và `f` (gộp `-rf`, `-fr`, `-r -f`, `--recursive --force`) |
| `git_force_push` | `git push` có `--force`, `-f`, `--force-with-lease`, hoặc refspec bắt đầu bằng `+` |
| `git_hard_reset` | `git reset --hard`, `git clean -fd…` |
| `pipe_to_shell` | segment trước là `curl`/`wget`/`fetch` và segment sau là `sh`/`bash`/`zsh`/`python…`/`node` |
| `sql_destructive` | `DROP TABLE|DATABASE|SCHEMA`, `TRUNCATE` (không phân biệt hoa thường) |
| `read_secret` | Read/Grep/Bash(cat, less, head, tail…) trên đường dẫn khớp `.env`, `.env.*`, `*.pem`, `*.key`, `id_rsa*`, `id_ed25519*`, `.ssh/`, `.aws/credentials`, `**/secrets/**` |
| `write_outside_workspace` | Edit/Write/MultiEdit có `file_path` tuyệt đối không nằm dưới `cwd` (sau khi chuẩn hóa `..`) |
| `outbound_network` | `curl`, `wget`, `nc`, `ncat`, `ssh`, `scp`, `rsync`, `ftp` tới host không phải `localhost`/`127.0.0.1`/`::1` |
| `privilege` | `sudo`, `su `, `chmod 777`, `chown -R` |

**`risk.rs`**
```
Critical  nếu khớp bất kỳ luật danger
High      nếu có Suspicion (HiddenChars, Subshell, Eval, Encoded) hoặc cài package (npm i, pnpm add, pip install, cargo add/install, brew install, apt)
Medium    git push (không force), migration, ghi file trong workspace, lệnh không nhận diện
Low       chỉ đọc trong workspace, chạy test/lint/build (npm test, pnpm test, cargo test, cargo clippy, pytest, go test)
```
`reversible = true` chỉ khi Low hoặc là migration có lệnh rollback được khai trong config (v0: chỉ Low).
`danger_spans`: vị trí khớp trong `Normalized.text` để UI tô sáng, theo đơn vị ký tự Unicode (§3.4).

**Luật bổ sung từ F6 (T5d · ARC-44, biện pháp B2):** `arcturus_ui_token`: mọi tool (Bash, Read, Grep, Edit, Write…)
có `tool_input` chạm đường dẫn token UI (`arcturus/ui-token`, `arcturus\ui-token`, file đặt bằng `--ui-token-file`) hoặc
gọi WebSocket/HTTP của hub (`/ws`, `/approvals/`, `127.0.0.1:<cổng hub>`, `localhost:<cổng hub>`) → **Critical**, không
bao giờ tự duyệt. Test: ≥3 ca khớp (`cat %APPDATA%\arcturus\ui-token`, `curl http://127.0.0.1:7878/ws`, `Read` file
token) và ≥2 ca không khớp. Lệnh đã được người dùng allow sẵn không đi qua hook nên luật này không chặn được chúng (xem
giới hạn ở `docs/ARCHITECTURE.md`).

**Bất biến:** `Policy::decide` với risk Critical **luôn** trả `AskUser`. Viết test thuộc tính (property-style, lặp
qua toàn bộ bảng mẫu × mọi trust grant giả và batch rule giả).

**Test bắt buộc:** mỗi luật ≥3 ca khớp và ≥2 ca không khớp (ví dụ `rm -r dir` không phải critical, `git push origin main` là medium);
`zero_width_inside_rm_still_detected`; `bidi_override_flagged`; `quoted_semicolon_not_split`;
`critical_never_auto_allowed_with_any_grant_or_batch`;
`danger_spans_are_char_indices_with_non_ascii` (ví dụ `echo "🚀 thư mục" && rm -rf đầu-ra`: ký tự tiếng Việt và
ký tự ngoài BMP (🚀) nằm **trước** chỗ khớp, nên span theo byte, theo UTF-16 và theo ký tự đều khác nhau; test khẳng
định span theo ký tự). T12 dùng lại đúng chuỗi này cho test tô sáng phía UI.

**Đã làm ở T8** (lệch so với trên, cần người duyệt khi review PR)
- Hai dạng của lệnh: `command_normalized` trên thẻ vẫn là dạng **hiển thị** của T5a (ký tự ẩn hiện `⟨U+XXXX⟩`, không bỏ),
  để người duyệt thấy chúng (bất biến 3); dạng bỏ ký tự ẩn/gộp khoảng trắng của bước 1–2 chỉ dùng nội bộ để so luật.
  `danger_spans` đổi từ dạng so khớp → chỉ số ký tự lệnh gốc → dạng hiển thị (`normalize::Display::span`).
- `Normalized.segments` là `Segment` (từ đã bỏ nháy/`\`, vị trí byte, độ sâu) thay vì `String`; thêm `invocations` (bỏ
  gán biến, từ khóa, lệnh bọc `sudo`/`env`/`xargs`/`timeout`…, cộng `find -exec`), `unparsed`, `too_long`. Nội dung trong
  dấu nháy được phân tích lại tới độ sâu 4 (`bash -c "…"`, `ssh host '…'`) **chỉ cho luật danger**; rủi ro chỉ tính lệnh
  ở độ sâu 0 (`git commit -m "fix bug"` không phải lệnh `fix`).
- Bảng ký tự ẩn chuyển từ `hub/src/view.rs` sang `hub_core::policy::normalize`, thêm Cf, Co, Cn đầy đủ theo
  `unicode-general-category` (Unicode 16; thư viện mới, lý do: mục còn mở ở T5a). Khoảng trắng Unicode, tab, Zl/Zp, Hangul
  filler → dấu cách khi so; Cc/Cf/Co/Cn/DI khác → bỏ.
- Luật mở rộng (bất biến 2 cho phép): PowerShell `Remove-Item -Recurse -Force`, cmd `rmdir /s /q`, `find -delete`;
  `git push --mirror/--delete/--prune`, refspec `:ref`, cụm cờ ngắn có `f`/`d`; `git clean -f` (không cần `d`);
  `dropdb`; tên bí mật thêm `.netrc`, `.git-credentials`, `.pgpass`, `.npmrc`, `.pypirc`, `*.p12`, `*.pfx`, `*.keystore`,
  `.gnupg/`, `.docker/config.json`, `.kube/config`, glob khớp chúng (`cat .e*`); bí mật xét ở **mọi** đối số Bash (cả `cp
  .env`, `--env-file=.env`) và đường dẫn của mọi tool trừ Glob; `write_outside_workspace` cho Bash (`>`, `>>`, `&>`, `tee`,
  `cp`/`mv`/`install`/`ln`/`rsync` đích, `dd of=`), so theo chữ sau `~`/`$HOME`/`$PWD`/`..`, đường dẫn Git Bash `/c/…`,
  không phân biệt hoa thường trên Windows; biến lạ (`$OUT`) là ngoài workspace; `outbound_network`: host lấy từ URL,
  `user@host`, `host:path`, proxy `-x`, jump `-J`; không tìm được host (`curl $URL`) là ngoài máy; `pipe_to_shell` gồm
  `bash <(curl …)`, `sh -c "$(curl …)"`, `eval "$(curl …)"`, `irm | iex`, và cả tải về localhost; `privilege` thêm
  setuid/setgid, `doas`, `pkexec`, `runas`; `sql_destructive` chạy cả trên chuỗi của tool MCP (không trên nội dung
  Edit/Write).
- `arcturus_ui_token` (B2) và luật mới `arcturus_state`: chuỗi con (bỏ ký tự ẩn, không phân biệt hoa thường, `\` = `/`)
  `arcturus/ui-token`, `ui-token` (có biên), `<host loopback>:<cổng hub>`, URL loopback tới `/ws`, `/approvals/`,
  `/hooks/` (cổng bất kỳ, kể cả biến), file `--ui-token-file`, `arcturus/arcturus.toml`, `arcturus/arcturus.db`, file cấu
  hình đang dùng; glob trong lệnh khớp các file đó (`~/.local/share/arc*/ui*`, `find -name 'ui-tok*'`). Xét trên mọi chuỗi
  của mọi tool, kể cả nội dung Write (script ghi ra rồi chạy sau). Luật `unanalyzed`: lệnh dài hơn 64 KiB không tách từ
  (chỉ chạm khi nới `handoff_max_bytes`) là Critical.
- Rủi ro: High thêm "tên lệnh là biến", mã viết thẳng (`python -c`, `node -e`, `bash -c`, `pwsh -c`…), `npx`/`pnpm dlx`/
  `uvx`/`go run`; "không phân tích được" (nháy chưa đóng, lệnh rỗng) là High theo ARC-29. Low: thêm `git status/log/diff/
  show/…`, `cargo fmt --check`, `eslint`/`ruff check` không `--fix`; ghi bằng chuyển hướng hay đối số đường dẫn ngoài
  workspace → không còn Low. Read/Grep/Glob trong workspace là Low, ngoài là Medium; Edit/Write trong workspace là
  Medium; WebFetch, Task, tool MCP là Medium (tool MCP có trường `command`/`cmd`/`script` thì xét trường đó như lệnh).
  Ký tự ẩn ở bất kỳ trường nào → `hidden_chars`, ít nhất High.
- `Policy` (trait) chuyển vào `hub_core::policy`: `decide(&ctx, &Assessment)`; `assess` (thuần) tách khỏi `decide`.
  `enforce` là bất biến chung: risk không thấp hơn đánh giá, tự duyệt chỉ tới Medium (Critical **và High** không bao giờ),
  đếm ngược chỉ khi `reversible`. Hub gọi `enforce` cho mọi kết quả, kể cả policy giả trong test.
  `RulesPolicy` = danger list → các `Allowance` (T9 batch, T10 trust cắm vào) → hỏi người; T8 chưa có allowance nào.
- Thẻ "duyệt trong terminal": `suspicions` tính bằng `quick_suspicions` (regex trên chữ, không tách từ, lệnh tới 1 MB).
- `hub --config` mặc định `<config_dir>/arcturus/arcturus.toml` thay vì `./arcturus.toml` (mục P3 ở T5b, ghi chú
  ARC-43/44 trong `.squad/TASKS.md`); thẻ PowerShell hiện `command` nguyên văn như Bash.
- Đã biết, chưa làm: so đường dẫn theo chữ (không theo symlink); WebFetch tới host ngoài là Medium (không thuộc bảng
  `outbound_network` vốn chỉ cho lệnh); `python -c "shutil.rmtree('/')"` chỉ là High (mã thông dịch không được phân tích).

### T9 · ARC-30 · Gom nhóm và batch rule

**`group_key`**: `"<tool>:<pattern>"`, với Bash là hai token đầu của segment đầu sau chuẩn hóa, bỏ đường dẫn
(`npm test -- auth` → `Bash:npm test`; `cargo test -p hub` → `Bash:cargo test`); tool khác là `"<tool>:*"`.

- `DecideGroup{group_key, decision}`: áp quyết định cho mọi pending cùng `group_key` **có risk ≤ High**; Critical trong nhóm vẫn ở lại hàng đợi (UI hiển thị riêng).
- `BatchOpen{group_key, minutes}` (1–240): lưu `batch_rules`; trong thời hạn, `Policy::decide` trả `AutoAllow` cho yêu cầu khớp và risk ≤ Medium.
- `BatchClose` hoặc hết hạn: xóa rule; broadcast `Feed`.

**Test:** `group_key_examples` (bảng), `decide_group_skips_critical`, `batch_rule_auto_allows_until_expiry`,
`batch_rule_never_applies_to_high_or_critical`, `batch_close_stops_auto_allow`.

**Đã làm ở T9**
- `hub-core/src/policy/group.rs`: `group_key(tool, tool_input)` đúng bảng ví dụ của đặc tả, thêm: bỏ đường dẫn và chữ hoa, lệnh bọc
  đứng trước (`sudo apt update` ≠ `apt update`), **lệnh ghép** mang hậu tố ` (ghép)` (nhiều lần gọi/đoạn ở đoạn chính, `$(…)`,
  cờ nghi ngờ, không phân tích được, tên chương trình động). Lý do: batch "cho phép `cargo test`" không được kéo theo
  `cargo test && npm install x`. Thẻ và nhật ký duyệt giờ ghi `group_key` thật (trước đây chỉ là tên tool).
- `hub-core/src/policy/batch.rs`: `BatchRules` (đồng hồ thay được để test), `Allowance` cắm vào `RulesPolicy`: tự duyệt khi trùng
  khóa, còn hạn, risk ≤ Medium; 1–240 phút, tối đa 64 quy tắc, khóa ghép/rỗng/quá dài/có ký tự điều khiển bị từ chối; `restore`
  không tin hàng đã lưu (hết hạn, khóa ghép, hạn dài hơn trần).
- `Approvals::decide_group`: mọi yêu cầu cùng nhóm trừ Critical, mỗi yêu cầu qua `Approvals::decide` (nên qua `allow_gate`, đúng ghi
  chú audit ARC-44 finding 8), chỉ `origin: user`; trả `GroupResult {matched, decided, held, critical_left}`.
- Hub: `Hub::batch_open/batch_close/restore_batch`, `Hub::start_with_batch` (tiêm bộ quy tắc; `start_full` tự dựng và cắm);
  quy tắc lưu ở SQLite (migration **v2** chỉ có `batch_rules`; bảng của T10 sẽ là migration sau, lệch nhẹ so với "v2 (T9, T10)" để
  hai task không đụng nhau); hết hạn báo bằng một task ngủ tới hạn (không polling), gia hạn thì task của hạn cũ bỏ qua; `main` nạp
  lại quy tắc còn hạn khi khởi động.
- WebSocket: `decide_group`, `batch_open`, `batch_close` hết "chưa hỗ trợ"; chung giới hạn 20 lệnh/giây với `decide`.
- UI: nút "Duyệt cả N" gửi **một** `decide_group` (hết trường hợp gửi dở nửa chừng của #13); hub trả `feed` kết quả.
- Test: 8 (`group.rs`) + 8 (`batch.rs`) + 2 (store) đơn vị và 13 tích hợp (`hub/tests/batch.rs`: bỏ Critical, qua `allow_gate`, thẻ cắt
  chưa mở, tự duyệt tới hạn, hạn theo đồng hồ giả, Critical/lệnh ghép không bao giờ, đội tạm dừng, đóng, sống qua khởi động lại,
  một timer và gia hạn, qua WebSocket).
- Chưa làm: UI mở/đóng batch và danh sách quy tắc đang mở (Snapshot chưa có trường này, cần đổi hợp đồng §3.4).

### T10 · ARC-31 · Trust grant

- `subject`: tên persona (ổn định qua phiên) — không dùng session_id.
- `pattern`: chính `group_key`.
- Sau mỗi quyết định của **người** cho yêu cầu risk ≤ Medium:
  - Allow → `approved_in_row += 1`, cập nhật `last_at` (đặt `first_at` nếu rỗng).
  - Deny → `approved_in_row = 0`, `last_incident_at = now`; nếu đang có grant cho (subject, pattern) → hạ một mức (6→5, 5→4 tức là thu hồi), ghi `trust_log`, broadcast `Feed`.
- Khi `approved_in_row >= streak_threshold` và `now - first_at <= streak_window_days` và không có incident trong cửa sổ → gửi `Suggestion` (một lần cho mỗi cặp cho tới khi được trả lời).
- `SuggestionAnswer{accept: true}` → tạo grant mức 6, hạn `grant_days`; `accept: false` → không đề xuất lại trong 7 ngày.
- `Policy::decide`: grant còn hạn mức 6 và risk ≤ Medium → `AutoAllow{reason: "trust:<id>"}`; mức 5 → trả `AskUser{countdown: Some(10s)}` (hub đặt `countdown_ms` theo §3.4; UI hiển thị đếm ngược, hết giờ UI gửi
`Decide{Allow, origin: Countdown}`; đội tạm dừng thì không có đếm ngược). Nếu không có UI nào đang mở thì không ai gửi Allow → hết giờ hub trả `{}` (an toàn).
- Hoàn tác (revert) chưa đo được tự động trong v0: để API `trust.report_incident(subject, pattern)` cho bước sau.

**Test:** `suggests_after_threshold_within_window`, `no_suggestion_if_incident_in_window`, `accept_creates_grant_level_6`,
`deny_under_grant_demotes`, `grant_expires`, `critical_ignores_grant` (dùng lại bảng của T8),
`level5_no_countdown_while_paused`, `level5_no_countdown_when_not_reversible`.

**Đã làm ở T10**
- `hub-core/src/policy/trust.rs` (thuần, đồng hồ thay được): `TrustBook` với `record`, `report_incident`, `answer`, `restore`,
  `grant_level`, và `Allowance`: mức 6 → `AutoAllow{reason: "trust:<id>"}`, mức 5 → `Countdown(10 s)`; chỉ tới Medium (Critical/High không
  bao giờ; `enforce` lọc lại). `subject` là tên persona (`ApprovalCtx.subject`, hub truyền `agent_name`), `pattern` là `group_key` (T9).
- Chuỗi chỉ đếm quyết định của **người** (`decided_by = user`) cho yêu cầu risk ≤ Medium và khóa không ghép; duyệt do đếm ngược/policy/
  batch không đếm. Chuỗi cũ hơn `streak_window_days` bắt đầu lại (đặc tả chỉ nói "trong cửa sổ", không nói chuỗi chậm thì sao).
  Đề xuất: đủ ngưỡng, không sự cố trong cửa sổ, chưa có mức 6, chưa có đề xuất chờ, không bị từ chối trong 7 ngày; từ chối chỉ nhớ
  trong bộ nhớ (mất khi khởi động lại, tránh thêm cột vào hợp đồng §3.5). Mức 5 chỉ tạo ra bằng hạ mức (6 → 5 khi bị từ chối); từ
  chối nữa thu hồi. `report_incident(subject, pattern)` có sẵn cho bước hoàn tác sau này.
- Giao thức: `ServerMsg::Suggestion{TrustSuggestionView}` (hub dựng `text` bằng template, hiện ký tự ẩn); hub gửi lại các đề xuất đang
  chờ ngay sau mỗi `Snapshot`; `SuggestionAnswer` hết "chưa hỗ trợ". Hạ mức/thu hồi/cấp quyền báo bằng `Feed` broadcast.
- Store: migration **v3** (`trust_grants`, `streaks`, `trust_log` đúng §3.5); `StoreReader::trust_grants/streaks/trust_log`; hub nạp lại
  khi khởi động (grant hết hạn/đã thu hồi/mức lạ bị bỏ).
- Hub: `Automation {batch, trust}` + `Automation::policy()` (batch trước, trust sau); `Hub::start_with_batch` nhận `Automation`.
- UI: `SuggestionList` ở popup và Văn phòng ("Cho phép"/"Để sau"), store giữ `suggestions`, `suggestion_answer` một thông điệp.
- Test: 12 (`trust.rs`) + 2 (store, protocol) đơn vị, 9 tích hợp (`hub/tests/trust.rs`: ngưỡng và phát một lần, chỉ người đếm, cấp grant mức 6
  chỉ cho đúng persona/nhóm, từ chối 7 ngày, hạ 6→5→thu hồi kèm `trust_log`, đếm ngược mức 5 và đội tạm dừng, Critical/High bỏ qua
  grant, sống qua khởi động lại, qua WebSocket) và 5 UI.
- Chưa làm: UI thu hồi grant thủ công và danh sách grant; hoàn tác tự động (chờ đo được); `declined_until` bền vững.

### T11 · ARC-32 · Component nhân vật

`ui/src/components/Character.tsx`:
```ts
type Mood = 'idle' | 'working' | 'approval' | 'error' | 'done';
interface CharacterProps { mood: Mood; width?: number; hair?: string; outfit?: string; acc?: string; label?: string }
```
- Port SVG ở **Phụ lục A**; mỗi nhóm `<g>` theo mood hiển thị bằng điều kiện React (không dùng opacity).
- `role="img"`, `aria-label` (mặc định theo mood: "Đang rảnh", "Đang làm việc", "Giơ tay xin duyệt", "Gặp lỗi", "Vừa xong việc").
- Không animation CSS vô hạn. Cho phép một hiệu ứng ngắn (≤600 ms) khi đổi sang `approval` hoặc `done`, tắt khi `prefers-reduced-motion`.

**Test:** render đủ 5 mood (kiểm phần tử đặc trưng có mặt), `aria-label` đúng, màu prop được áp.

**Đã làm ở T11**
- `Character.tsx` + `Character.css`; xuất thêm `Mood`, `MOOD_LABELS`, `DEFAULT_PALETTE` (bảng màu đầu của phụ lục B,
  dùng khi thiếu prop). `width` mặc định 120. T12 có thể chuyển `Mood` sang `protocol.ts`.
- Phần vẽ có `data-part` (`eyes-<mood>`, `mouth-*`, `raised-hand`, `alert-badge`, `sparkles`, `typing-bubble`, `sleep-z`,
  `sweat-drop`, `hair-*`, `outfit`, `headphones`) để test và UI sau này nhắm tới; svg có `data-mood`.
- Màu chỉ nhận `#RRGGBB`, giá trị khác (rỗng, `url(…)`, tên màu) dùng màu mặc định: màu đi thẳng vào thuộc tính SVG.
- Hiệu ứng: nhóm phụ kiện phóng lên 450 ms, một lần, chỉ khi **chuyển sang** `approval`/`done` (lần render đầu không
  chạy, render lại cùng mood không chạy lại); `prefers-reduced-motion` tắt hẳn.

### T12 · ARC-33 · Tray và popup Lite

**Tauri (`crates/app/src-tauri`)**
- Không mở cửa sổ khi khởi động; chỉ tạo tray icon.
- Click trái tray: nếu popup chưa có → tạo `WebviewWindow` nhãn `popup` (380×560, không viền, luôn ở trên, đặt gần khay);
  nếu có → đóng (**destroy**, không hide).
- Mất focus → đóng popup.
- Menu tray: "Mở văn phòng", "Tạm dừng cả đội", "Thoát".
- Badge: macOS dùng tiêu đề tray (số); Windows/Linux đổi giữa hai ảnh icon (thường / có chấm) và tooltip "N yêu cầu đang chờ".
  Số lấy từ một kết nối WebSocket nhẹ ở phía Rust của app (không cần webview).
- Token cho webview: app **khởi động hub** làm tiến trình con với `--ui-token-stdout`, đọc token UI từ dòng đầu stdout
  (không có file token trên đĩa, F6 biện pháp B1), rồi truyền qua `invoke` (không nhúng vào URL công khai). Không dùng
  `ARCTURUS_TOKEN` (token của hook) cho UI. Nếu hub đã chạy sẵn do người dùng tự khởi động: đọc file token như T5b.
  App phải **liên tục đọc (hoặc chuyển hướng) stderr** của hub: pipe đầy thì log chặn luồng của hub (audit ARC-44
  finding 5). Đọc stdout bằng pipe thật; thiếu stdout thì hub vẫn báo ghi thành công mà không ai nhận token.
- UI làm mờ nút Allow trong `min_review_ms` kể từ lúc **hiện** thẻ (kể cả thẻ tới qua `Snapshot`).

**UI (`views/LitePopup.tsx`)**
- Danh sách `ApprovalCard`: nhân vật 40px, tên, nguồn/máy, `command_normalized` (đầy đủ, không rút gọn; quy tắc bất biến 3) trong khối mono có tô sáng `danger_spans` (đơn vị ký tự Unicode, §3.4), nhãn rủi ro (chữ + màu; không dùng màu đơn thuần), thời gian chờ.
- `suspicions` có `hidden_chars` → nhãn cảnh báo "Chứa ký tự ẩn" (chữ + màu) và nút "Xem bản gốc": hiện
  `command_original` với mỗi ký tự vô hình/điều khiển hướng thay bằng mã nhìn thấy được, ví dụ `⟨U+200B⟩`
  (hiện nguyên ký tự ẩn thì người duyệt vẫn không thấy).
- `truncated = true` → dòng "Nội dung đã cắt" và nút "Xem đầy đủ" gọi `GET /approvals/{request_id}/input`.
- Nhóm cùng `group_key` có ≥2 yêu cầu → hiện "Duyệt cả N" (ẩn với Critical).
- Phím: ↑/↓ chọn, Enter đồng ý, Esc từ chối (trong popup, không toàn cục).
- Nếu Q1 = terminal bị treo: thêm nút "Trả về terminal" gửi `ReturnToTerminal` (hub trả `{}` ngay).

**Test:** reducer áp snapshot/delta đúng; `ApprovalCard` hiển thị `command_normalized` và tô sáng đúng vị trí, kể cả lệnh có ký tự ngoài ASCII/ngoài BMP; phím tắt gửi đúng `ClientMsg`;
nhóm Critical không có nút duyệt gộp; `hidden_chars` hiện nhãn cảnh báo và "Xem bản gốc" hiện mã `⟨U+…⟩`;
`truncated` hiện nút "Xem đầy đủ". Đo tay: popup mở ≤300 ms, RSS app sau khi đóng ≤60 MB (ghi vào BUDGET.md).

**Đã làm ở T12** (lệch so với trên, cần người duyệt khi review PR)
- Hub thêm `--exit-on-stdin-close`: app giữ pipe stdin của hub con; app thoát hay crash thì hub tắt êm như Ctrl-C (trả
  `{}` cho mọi yêu cầu đang giữ), không mồ côi giữ cổng với token UI không ai biết. Menu "Thoát" cũng tắt hub con. Hub
  người dùng tự khởi động (đã có trên cổng) thì app chỉ kết nối bằng file token, thoát app không tắt nó.
- **Lỗi tìm thấy khi chạy thật, đã sửa trong hub**: khi stderr của hub gãy (app đã chết, terminal đã đóng),
  tracing-subscriber báo lỗi ghi log bằng `eprintln!`, mà `eprintln!` panic, giết task đang ghi log. Thực tế: future tắt
  hub chết, yêu cầu đang giữ treo tới hết giờ thay vì nhận `{}`. Sửa: `log_internal_errors(false)` (không ghi được thì bỏ
  dòng log). Test hồi quy `broken_stderr_does_not_block_shutdown`.
- App: `hub_proc.rs` (tìm hub ở `ARCTURUS_HUB_ADDR`, mặc định `127.0.0.1:7878`, chỉ loopback; binary hub ở
  `ARCTURUS_HUB_BIN`, cạnh binary app, `target/{debug,release}` khi build debug, rồi `PATH`; token file đổi được bằng
  `ARCTURUS_UI_TOKEN_FILE`), `link.rs` (WS phía Rust: badge + tạm dừng/tiếp tục đội từ menu), `http.rs` (GET tối giản
  cho `/healthz` và `/approvals/{id}/input`), `tray.rs`. Lệnh `invoke`: `hub_info`, `approval_input` (kiểm `req_` + chữ
  số trước khi ghép đường dẫn). Hub con cần `ARCTURUS_TOKEN` trong môi trường của app; thiếu thì tooltip báo dòng lỗi
  cuối của hub.
- Linux (appindicator) không báo sự kiện click tray: thêm mục menu "Mở hàng đợi duyệt" (mọi nền tảng). Popup không có
  vị trí icon thì đặt ở góc trên phải màn hình chính.
- Badge đếm cả thẻ duyệt và thẻ "duyệt trong terminal" (đều đang chờ người); tooltip ghi riêng hai số.
- "Duyệt cả N": T9 chưa có `decide_group` ở hub, nên UI gửi từng `decide` (hub vẫn kiểm `allow_gate` cho từng yêu cầu).
  Nhóm chỉ gồm yêu cầu không Critical (khớp luật T9 "risk ≤ High"); nhóm còn dưới 2 thì không có nút. Nút chỉ bật khi
  mọi yêu cầu trong nhóm đã qua sàn thời gian và (nếu cắt) đã mở bản đầy đủ.
- Sàn thời gian ở UI dùng hằng 1.000 ms (mặc định `min_review_ms` của hub): giao thức chưa gửi giá trị cấu hình. Hub vẫn
  tự giữ allow sớm hơn sàn của nó. Đề xuất (ticket mới, làn hub): thêm `min_review_ms` vào `TeamView`.
- Không có nút "Trả về terminal" (Q1: terminal không treo); thẻ ghi "Cũng đang hỏi trong terminal của agent".
- UI: `protocol.ts` viết tay, `store.ts` (reducer), `ws.ts` (`HubClient`, backoff 0,5–30 s, lấy lại token mỗi lần kết
  nối), `hub.ts` (`invoke`; chạy UI riêng trong trình duyệt khi phát triển bằng `VITE_HUB_WS_URL`, `VITE_HUB_TOKEN` với
  hub bản debug), `useNow` (timer 250 ms chỉ khi có yêu cầu đang chờ và trang đang hiện), `ApprovalCard`, `HandoffCard`,
  `QueueList`, `views/LitePopup`; `?view=office` là chỗ giữ cho T13. Phông Be Vietnam Pro, JetBrains Mono qua
  Fontsource (đóng gói vào bundle, không tải từ mạng).
- `tauri.conf.json`: không cửa sổ nào lúc khởi động; CSP chặt (`connect-src` chỉ IPC và `ws://127.0.0.1:*`,
  `ws://localhost:*`). Capability áp cho cửa sổ `popup`, `office`.
- Chưa đo được trong container (không có tray thật, không có GPU): popup mở ≤300 ms và RSS sau khi đóng popup; số đo
  hiện có ở `docs/BUDGET.md`, cần đo lại trên máy chuẩn Windows.

**Đã làm thêm ở T12 khi đo bằng T16 (2026-10-27): prewarm khi rê chuột**
- Đo bản release trên Windows: tạo webview WebView2 nguội mất ≈285 ms, nên popup mở ≈340–400 ms kể từ lúc bấm (không đạt ≤300 ms). Cờ
  `--disable-gpu`/`--in-process-gpu` chỉ giảm ≈50 ms nên không dùng.
- `tray.rs`: con trỏ vào icon (`TrayIconEvent::Enter`, và `Move` vì Windows chỉ phát `Leave` sau ít nhất một `Move`) và dừng ≥150 ms thì tạo
  sẵn popup **ẩn** (`build_popup`); bấm thì chỉ `show_popup`. Sau một cú bấm không tạo sẵn lại cho tới lần rê sau. Rời icon mà không bấm
  trong 3 s, hoặc 8 s kể từ lúc tạo dù không có `Leave`, thì destroy (RSS về mức Lite). Click trái, mất focus, đóng và đường nguội
  không đổi. Đo: bấm 500 ms sau lúc con trỏ vào icon thì khung đầu tiên sau trung vị 44 ms (tối đa 66 ms); bấm sớm hơn thì chờ phần còn lại
  của lần tạo (≈200 ms nếu bấm sau 300 ms, ≈350 ms nếu bấm ngay). Quy tắc "không giữ webview khi popup đóng" giờ có ngoại lệ ngắn
  (≤ 8 s sau khi rê chuột, tạm ≈360 MB WS).
- Test: `tray::tests` (4: điều kiện hẹn tạo sẵn, đồng hồ "UI vẽ xong" trước/sau lúc bấm), `measure::tests`; chế độ đo chạy qua đúng `hover_enter`/`hover_leave`
  (`-HoverMs 500`, kiểm cả popup bỏ dở bị hủy). Đã kiểm bằng chuột thật trên bản release (2026-10-27, cờ `ARCTURUS_MEASURE_LOG=1`): `Enter` thật kích hoạt prewarm, bấm → khung đầu tiên 12 ms; mở lại ngay sau khi đóng (con trỏ chưa rời icon) đi đường nguội 456 ms.

### T13 · ARC-34 · Văn phòng 2D

`views/Office.tsx` trong cửa sổ `office` (mở từ menu tray, destroy khi đóng).
- Lưới `AgentDesk` (CSS grid auto-fill, min 210px): ô nhân vật, nhãn trạng thái, tên, vai trò, nguồn, máy, thanh context.
- Sắp xếp: giơ tay → lỗi → đang làm → xong → rảnh; trong nhóm theo `last_event_at`.
- Đội hình chính: `roster_max` agent đầu; agent **rảnh** còn lại gom vào thẻ "Phòng hỗ trợ" (số lượng + chấm màu). Agent không rảnh không bao giờ bị gom.
- Bên phải: `QueueList` dùng lại `ApprovalCard`.
- Hiệu năng: không `requestAnimationFrame` thường trực; chỉ CSS transition ngắn khi đổi trạng thái. Lắng nghe `visibilitychange`: khi ẩn thì ngắt mọi timer UI.

**Test:** gom phòng hỗ trợ đúng (không gom agent đang làm), thứ tự sắp xếp, bấm agent đang giơ tay cuộn tới thẻ duyệt.

**Đã triển khai ở T13:** `views/Office`, `components/AgentDesk`, `officeRoster`; Office và popup dùng chung
`useHubConnection`/`useApprovalQueue`. Thứ tự trong nhóm: `last_event_at` mới nhất trước, id phá hòa.
Agent không rảnh (kể cả agent con) luôn có bàn riêng; chỉ phần idle vượt `roster_max` vào phòng hỗ trợ.
Chọn agent chuyển focus/cuộn đến yêu cầu cũ nhất hoặc thẻ handoff. Page Visibility dừng timer UI và hiệu ứng CSS.
Chi tiết thao tác, kiểm tra và giới hạn xác minh desktop: `docs/OFFICE.md`.

### T14 · ARC-35 · Lệnh cho cả đội

- Nút "Tạm dừng cả đội" → `TeamPause` (phần hub đã làm ở T3 và T5). Hub v0 **không** chặn agent (chưa có cơ chế
  dừng Claude Code từ ngoài một cách an toàn) — chỉ: (a) mọi `AutoAllow` bị tắt khi paused, (b) UI hiển thị trạng thái
  "đã tạm dừng". Ghi rõ giới hạn này trong UI tooltip và `docs/ARCHITECTURE.md`.
- "Giờ tập trung" → `FocusSet{on}`: ẩn **chấm báo và thông báo** (badge tray, tooltip; âm báo khi có, v0 chưa có) cho yêu cầu
  risk Low/Medium; High/Critical vẫn báo. **Không ẩn thẻ** trong popup/Office: yêu cầu đang chờ luôn hiện khi người dùng
  mở cửa sổ, vì ẩn thẻ là giấu lệnh đang chặn agent. Hub không đổi hành vi duyệt.
- Đếm ngược phủ quyết: hiển thị khi `ApprovalView.countdown_ms` có giá trị (hub chỉ gửi cho yêu cầu reversible, T10
  mức 5). Đồng hồ chạy tới `max(created_at, lúc thẻ hiện) + countdown_ms` (thẻ tới UI muộn qua `Snapshot` không tự chạy
  ngay khi vừa hiện; hub vẫn chỉ nhận khi `now ≥ created_at + countdown_ms`). Hết giờ: UI gửi
  `Decide{Allow, origin: Countdown}` đúng một lần, chỉ khi đã kết nối và trang đang hiện. Nút "Dừng lại" gửi Deny, "Chạy ngay" gửi Allow, cả hai
  `origin: User`. Không bao giờ hiển thị cho Critical/High.
- Nhận `TeamUpdated{paused: true}`: reducer xóa `countdown_ms` của mọi yêu cầu trong store và dừng mọi đồng hồ ngay
  (hub vẫn chặn ở phía mình, §3.4). Tiếp tục (resume) không khởi động lại đếm ngược cho yêu cầu cũ.

**Test:** nút tạm dừng gửi `TeamPause`/`TeamResume` và hiển thị theo `TeamView.paused` (phía hub đã test ở T3, T5);
focus lọc thông báo theo risk; countdown chỉ xuất hiện khi có `countdown_ms`; hết giờ gửi `origin: Countdown`, nút
bấm gửi `origin: User`; `paused` dừng mọi đồng hồ đang chạy và resume không chạy lại.

**Đã làm ở T14**
- `TeamControls` (popup và Office): "Tạm dừng cả đội"/"Tiếp tục cả đội" (`team_pause`/`team_resume`, `aria-pressed` theo
  `TeamView.paused`) và "Giờ tập trung" (`focus_set`). Tooltip nói rõ giới hạn v0: hub chỉ tắt tự duyệt và đếm ngược,
  không dừng được Claude Code. Nút tắt khi chưa kết nối; gửi lỗi thì báo.
- Giờ tập trung chỉ đổi **chấm báo và tooltip của tray** (`link.rs` đếm theo `risk`: Low/Medium ẩn, High/Critical, `risk`
  thiếu hoặc lạ, và thẻ "duyệt trong terminal" luôn báo; tooltip nói có bao nhiêu yêu cầu đang bị ẩn). Thẻ trong popup và
  Office **không bị ẩn** (đã chốt trong đặc tả ở trên; v0 chưa có âm báo). Thêm
  mục menu tray "Bật/Tắt giờ tập trung".
- Đếm ngược (`countdown.ts`, `useApprovalQueue`): chỉ hiện và chạy khi `countdown_ms` có giá trị **và** `reversible` **và**
  risk Low/Medium **và** đội không tạm dừng (UI tự kiểm lại, không tin hub một mình). Mốc hết giờ tính từ
  `max(created_at, lúc thẻ hiện) + countdown_ms` (xem đặc tả: không bao giờ ngắn hơn điều kiện hub). Hết giờ gửi
  `decide{allow, origin: countdown}` đúng một lần cho mỗi yêu cầu, chỉ khi đã kết nối và trang đang hiện. "Dừng lại"
  (`deny`) và "Chạy ngay" (`allow`, vẫn qua `allowGate`) gửi `origin: user`.
- Reducer: `team_updated{paused: true}`, `snapshot` đang tạm dừng và `approval_requested` khi tạm dừng đều đặt
  `countdown_ms = null`; resume không khôi phục.
- Chưa làm: âm báo (chưa có trong v0), đo lại ngân sách app (chỉ thêm đếm trong `link.rs`, không thêm timer).

### T15 · ARC-36 · Replay

`tools/replay`: `replay <dir> --url http://127.0.0.1:7878 --sessions N --rate R/s --duration D [--permission-mode allow|skip]`.
- Đọc fixture, thay `session_id` bằng `replay-<i>`, gửi tới `/hooks/event` (và `/hooks/permission` nếu bật; đóng vai người
  trả lời qua WebSocket với chế độ `allow`).
- In p50/p95/p99 độ trễ và số lỗi; mã thoát ≠0 nếu p99 > 5 ms cho `/hooks/event` (cờ `--assert-p99-ms`).

**Đã làm ở T15**
- `tools/replay` tách thành thư viện (`files`, `send`) và binary; `replay [DIR]` giờ **gửi** tới hub (trước đây chỉ đếm; `--dry-run`
  giữ hành vi đếm). Cờ: `--url`, `--sessions` (mặc định 1), `--rate` (20/giây), `--duration` (5 giây), `--permission-mode
  allow|skip` (mặc định `skip`), `--allow-delay-ms` (1.100, phải lớn hơn `min_review_ms` của hub), `--assert-p99-ms`.
- Token: hook từ `ARCTURUS_TOKEN` (hoặc `--hook-token`); token UI (chỉ cần với `allow`) từ `ARCTURUS_UI_TOKEN`, `--ui-token`
  hoặc `--ui-token-file` (file do `hub --ui-token-file` ghi). Token không in ra.
- Kế hoạch gửi xác định: phiên `n % sessions`, mỗi phiên đi hết danh sách fixture theo thứ tự, lệch pha giữa các phiên.
  `PermissionRequest` đi `/hooks/permission`, còn lại `/hooks/event`; `skip` bỏ `PermissionRequest`.
- Chế độ `allow` kết nối WebSocket trước khi gửi (Snapshot cũng được duyệt) và gửi `decide{allow, origin: user}` sau
  `--allow-delay-ms`. Vì replay xen sự kiện của cùng phiên (`Stop`, `PostToolUse`…), luật "đã được trả lời ở terminal" của T3
  hủy nhiều yêu cầu giữ trước khi tới lượt duyệt: kết quả `{}` nhiều là bình thường, không phải lỗi.
- Báo cáo riêng cho `/hooks/event` và `/hooks/permission` (p50/p95/p99/max, lỗi, allow/deny/`{}`). Thoát ≠0 khi `/hooks/event`
  có lỗi hoặc p99 vượt `--assert-p99-ms`; lỗi ở `/hooks/permission` không làm hỏng kiểm p99.
- Test: 9 test đơn vị (phân vị, thay `session_id`, kế hoạch, đọc quyết định, báo cáo/kiểm ngưỡng, fixture thật) và 6 test tích hợp
  với hub thật (`tools/replay/tests/e2e.rs`: chỉ sự kiện, người duyệt giả, sai token hook/UI, thiếu token UI, tùy chọn sai).
- Chạy tay với hub release: 5 phiên, 200 yêu cầu/giây, 5 giây, 952 sự kiện: p50 0,45 ms, p95 0,68 ms, p99 0,93 ms, 0 lỗi.

### T16 · ARC-37 · Đo ngân sách

`scripts/measure.sh` (Linux/macOS) và `scripts/measure.ps1` (Windows) theo kịch bản `docs/BUDGET.md`; xuất CSV
`ts,rss_kb,cpu_pct` và một dòng tóm tắt. Agent `budget-guard` dùng các script này.

**Đã làm ở T16** (lệch so với trên: thêm hai cột CSV và chế độ đo trong app)
- `scripts/measure.ps1` (hub và app) và `scripts/measure.sh` (hub; app Tauri chưa đo được trên Linux/macOS ở v0). CSV `ts,rss_kb,cpu_pct,priv_kb,phase`
  (thêm bộ nhớ riêng và tên pha); một dòng tóm tắt `measure target=… verdict=PASS|FAIL`, mã thoát 1 khi vượt ngân sách. Phần phân tích tách
  riêng (`-Analyze <csv>` / `--analyze <csv>`) và có test bằng CSV giả: `scripts/test-measure.ps1` (12 kiểm tra), `scripts/test-measure.sh` (9).
- Hub: khởi động hub release trên cổng riêng với CSDL tạm, idle 60 s, `replay` 20 phiên 50 yêu cầu/s trong 120 s (`--permission-mode skip`), nghỉ 60 s;
  chỉ đo tiến trình hub (con của hub như `conhost` hay `claude agents --json` của reconcile không tính). p99 lấy từ đầu ra của replay.
- App: không thể bấm tray tự động, nên `crates/app/src-tauri/src/measure.rs` thêm chế độ đo, chỉ bật khi có `ARCTURUS_MEASURE_POPUP=<số lần>`
  (`_DELAY_SEC`, `_HOLD_SEC`): app tự mở/đóng popup và ghi `[measure] t=… event=…` ra stderr. `popup_ready` là thời gian từ lúc bắt đầu `open_popup` tới khi UI
  gọi lệnh Tauri `popup_ready` (một `requestAnimationFrame` sau khi `LitePopup` mount; ngoài chế độ đo là lệnh rỗng). Test: `measure::tests` (3) và `ui/src/hub.test.ts` (3).
- Kết quả đo thật 2026-10-27 ở `docs/BUDGET.md`: hub đạt; app tray và sau đóng đạt, popup mở vượt (WS 367 MB, 396 ms).

### T17 · ARC-38 · E2E với Claude Code thật

`crates/hub/tests/e2e_claude.rs`, đánh dấu `#[ignore]` (chạy tay hoặc với biến `ARCTURUS_E2E=1`).
- Tạo thư mục tạm có `package.json` với script `test` vô hại; settings cục bộ trỏ hook về hub test.
- Chạy `claude -p "chạy npm test"` với permission mode buộc hỏi; 4 kịch bản: allow, deny, timeout (timeout hub 5 s), hub tắt.
- Kiểm: file đánh dấu do lệnh tạo ra có/không xuất hiện đúng theo kịch bản.

**Đã làm ở T17**
- `crates/hub/tests/e2e_claude.rs`, 4 test `#[ignore]` (chạy tay: `cargo test -p hub --test e2e_claude -- --ignored --nocapture
  --test-threads=1`; biến tùy chọn `ARCTURUS_E2E_MODEL`, `ARCTURUS_E2E_CLAUDE`; tự bỏ qua nếu không có `claude`). Hub chạy trong tiến
  trình test (cổng ngẫu nhiên, policy thật `RulesPolicy`, không store); dự án tạm có `npm test` ghi `marker.txt`; hook về hub nạp bằng
  `--settings` (không đụng `~/.claude`), token qua biến môi trường như bản dogfood; `claude -p ... --permission-mode default`.
- Kết quả **chạy thật trên Claude Code 2.1.308 (Windows, model haiku), 4/4 đạt, 39 giây**:
  1. allow (người duyệt giả trả `allow` sau 300 ms): hub thấy `PermissionRequest` của `npm test` và ghi `ApprovalResolved(allow, user)`; file
     đánh dấu **có**.
  2. deny: file đánh dấu **không**.
  3. timeout (hub 5 giây, không ai trả lời): `ApprovalResolved(decision = null, timeout)`, file **không**, tổng thời gian ≥ 5 giây.
  4. hub tắt (cổng không ai lắng nghe): `claude -p` không treo, file **không**.
  => Xác nhận trên máy thật quy tắc bất biến 1 cho **hết giờ và lỗi kết nối**: không bao giờ có allow; Claude Code rơi về hộp thoại
  terminal, ở headless không có người hỏi nên lệnh bị từ chối. Chưa kiểm: phản hồi non-2xx (`401` sai token, `400`) và `{}` khi hub trả
  sớm ở các nhánh `Overloaded`/hand-off; các nhánh đó đã có test ở phía hub.
- Lỗi của chính test đã sửa khi chạy: người duyệt giả chạy vòng `recv` trên kênh broadcast không bao giờ đóng (Hub còn sống) nên `await` chờ
  mãi; nay dừng bằng `abort` (`Responder::finish`).
- Bộ test không chạy trong CI mặc định (cần Claude Code đăng nhập, tốn lượt dùng).

### T18 · ARC-39 · Dogfood

- `hooks/arcturus-dogfood.json` hoàn chỉnh; `scripts/dogfood-install.sh` và `dogfood-uninstall.sh` gộp/gỡ khối `hooks`
  trong `~/.claude/settings.json` **có sao lưu** (`settings.json.bak-<ts>`), dùng `jq`; không đụng các khóa khác.
- `docs/DOGFOOD.md`: các bước, cách gỡ, cách kiểm tra hub tắt vẫn an toàn.

**Đã làm ở T18**
- `hooks/arcturus-dogfood.json`: `SessionStart` đổi từ hook `http` sang hook `command` + `curl` (hook http không chạy cho
  `SessionStart`, F1). Lệnh in ra stdout/stderr **không gì cả** và luôn thoát 0 (`-s -o /dev/null … || true`): stdout của
  hook `SessionStart` được đưa vào ngữ cảnh của Claude. Đã chạy lệnh đã cài với hub thật: đúng token thì `session.started`
  vào DB; hub tắt, sai hay thiếu token thì im lặng, thoát 0 trong 0,01 s và không có gì được lưu. `_comment` trỏ tới script.
- `scripts/dogfood-install.sh`, `dogfood-uninstall.sh`, `dogfood-lib.sh` (dùng chung): ngoài đặc tả ("gộp/gỡ khối `hooks`,
  có sao lưu, `jq`"), thêm `--settings FILE`, `--port N` (viết lại `127.0.0.1:7878`), `--dry-run` (in khác biệt, không ghi,
  không sao lưu), `CLAUDE_CONFIG_DIR`. Gộp **vào cuối mảng** của từng sự kiện (hook của người dùng đứng trước, không bị ghi
  đè); nhận diện hook của Arcturus theo đích `http://127.0.0.1:<cổng>/hooks/(event|permission)` (không thêm khóa lạ vào
  settings, vì Claude Code có thể từ chối); cài lại không nhân đôi, nội dung không đổi thì không ghi, không sao lưu; JSON
  hỏng hay `hooks` sai hình dạng thì dừng, không ghi; ghi bằng `>` để giữ symlink (dotfiles) và quyền file; ghi lỗi thì
  khôi phục từ bản sao lưu; gỡ chỉ bỏ mục/khóa trở nên rỗng vì việc gỡ.
- `scripts/test-dogfood.sh` (54 kiểm tra, chỉ dùng thư mục tạm, `HOME`/`CLAUDE_CONFIG_DIR` trỏ vào đó nên không đụng
  `~/.claude` thật) + job CI `scripts` (`shellcheck -x scripts/*.sh`, test). Kiểm đột biến: làm hỏng bản sao của script/hook
  (strip no-op, không sao lưu, ghi đè hook người dùng, bỏ đổi cổng, dry-run vẫn ghi, ghi qua `mv`, `SessionStart` về http,
  đích ngoài localhost, token cứng, timeout 600) thì test đỏ.
- `docs/DOGFOOD.md`: chuẩn bị, token, chạy app, cài, kiểm tra nối đúng, kiểm tra hub tắt vẫn an toàn, gỡ, giới hạn (F2, F6,
  lệnh quá dài, nhật ký SQLite chứa lệnh), cách ghi lại.
- **Chưa kiểm chứng** (cần máy có Claude Code đăng nhập; T17): các hook `http` ở trạng thái lỗi kết nối/hết giờ/non-2xx
  của Claude Code thật. Container này không có đăng nhập Claude, nên chỉ kiểm được phía hub và lệnh `curl`.

### T19 · ARC-40 · Standup

`standup.rs`: `fn build(store: &StoreReader, since: OffsetDateTime, now: OffsetDateTime, git: &dyn GitLog) -> Standup`.
- Theo persona: số task xong (TurnStopped), commit (từ `git log --since` trong cwd của persona), test pass/fail (PostToolUse Bash khớp mẫu test + exit code nếu có), yêu cầu đang chờ, lỗi lặp lại ≥3 lần.
- Văn bản sinh bằng template tiếng Việt cố định; không LLM.
- Endpoint `GET /standup` (Bearer) trả JSON; UI hiển thị ở v1.

**Test:** dữ liệu giả lập → đúng số liệu và câu chữ (snapshot).

**Đã làm ở T19**
- `hub-core/src/standup.rs` (thuần): `build(&Input, &dyn GitLog) -> Standup` thay cho chữ ký `build(store, since, now, git)` của đặc tả
  (hub đọc SQLite rồi đưa sự kiện vào, nên test không cần cơ sở dữ liệu). Đếm theo persona: `TurnStopped` = lượt xong;
  `PostToolUse` / `PostToolUseFailure` của Bash khớp bộ chạy test = test đạt / lỗi (không cần mã thoát: Claude Code chỉ bắn
  `PostToolUse` khi lệnh thành công); lỗi công cụ lặp ≥3 lần; yêu cầu đang chờ; commit từ `git log`.
- Persona của sự kiện: agent đang chạy (đúng tên đang hiện) → `match_cwd` → `Agent <4 ký tự session>`; các phiên cùng persona gộp.
- `StoreReader::events_since`; `Hub::set_db_path` (đặt một lần ở `main`) để `GET /standup` mở kết nối đọc khi cần.
- `hub/src/standup.rs`: `SystemGit` chạy `git -C <cwd> -c core.fsmonitor=false --no-pager log --no-merges --since=…` (timeout 3 giây, tối
  đa 256 KiB, thread đọc riêng để pipe không đầy; lỗi mọi kiểu trả rỗng); handler chạy trong `spawn_blocking`.
- `GET /standup?hours=` ở router UI (token UI, kiểm `Host`/`Origin`, `no-store`); mô tả ở `docs/PROTOCOL.md`.
- Rủi ro đã xử lý: tiêu đề commit và lệnh là dữ liệu do agent/repo viết nên đi qua `clean` (hiện ký tự ẩn, cắt); `cwd` do agent
  gửi chỉ làm đối số của `-C`, không bao giờ qua shell; chạy `git` không tương tác (`GIT_TERMINAL_PROMPT=0`).
- Test: 8 đơn vị (`standup.rs`: đếm, template bằng so khớp chuỗi, rỗng, làm sạch, tối đa 5 thư mục, nhận diện lệnh test) và 6
  tích hợp (`hub/tests/standup.rs`: SQLite + persona + hàng đợi thật, token UI/hook, `hours` sai, 503, `Host` lạ, `git log` thật).
- Chưa làm: UI hiển thị (v1 theo đặc tả); đếm "task xong" theo ticket (chỉ có lượt trả lời).

### T20 · ARC-41 · Chỉ số dogfood

`replay metrics` hoặc truy vấn SQL trong `scripts/metrics.sql`: thời gian chờ trung bình mỗi yêu cầu theo ngày,
tỉ lệ `decided_by=policy`, số deny, số timeout. Baseline (trước Arcturus) đo tay, ghi vào `docs/DOGFOOD.md`.

### T21 · ARC-42 · Cổng v0

Chạy `/milestone-gate v0`. Không code mới.

---

## Phụ lục A — SVG nhân vật (port sang `Character.tsx`)

viewBox `0 0 200 240`, chiều cao = width × 1.2. `H` = hair, `O` = outfit, `A` = acc.

```
Luôn có:
<ellipse cx=100 cy=231 rx=52 ry=7 fill=#000 opacity=.25/>
<path d="M36 104 Q32 38 100 34 Q168 38 164 104 L160 160 Q150 176 140 162 L140 122 L60 122 L60 162 Q50 176 40 160 Z" fill=H/>
<path d="M52 232 Q50 166 100 160 Q150 166 148 232 Z" fill=O/>
<path d="M84 163 L100 182 L116 163" fill=none stroke=#F4F1FF stroke-width=4 stroke-linejoin=round opacity=.85/>
<rect x=90 y=146 width=20 height=18 rx=6 fill=#F7D3C1/>
<ellipse cx=100 cy=104 rx=56 ry=52 fill=#FFE6D8/>
<path d="M42 100 Q44 44 100 42 Q156 44 158 100 Q146 78 128 80 Q122 64 106 76 Q94 60 80 76 Q62 70 56 88 Q48 90 42 100 Z" fill=H/>
<path d="M100 44 Q104 22 122 24 Q108 30 106 46 Z" fill=H/>
<path d="M40 104 Q38 30 100 28 Q162 30 160 104" fill=none stroke=A stroke-width=8 stroke-linecap=round/>
<rect x=28 y=92 width=18 height=34 rx=9 fill=A/>  <rect x=154 y=92 width=18 height=34 rx=9 fill=A/>
<ellipse cx=70 cy=122 rx=9 ry=5 fill=#FF8FA8 opacity=.55/> <ellipse cx=130 cy=122 rx=9 ry=5 fill=#FF8FA8 opacity=.55/>

Mắt:
approval: <ellipse cx=78 cy=106 rx=7 ry=10 fill=#241B4D/><ellipse cx=122 cy=106 rx=7 ry=10 fill=#241B4D/>
          <circle cx=80.5 cy=102 r=3 fill=#fff/><circle cx=124.5 cy=102 r=3 fill=#fff/>
done:     stroke #241B4D w4 round: <path d="M70 108 Q78 98 86 108"/><path d="M114 108 Q122 98 130 108"/>
working:  <ellipse cx=78 cy=110 rx=7 ry=6 fill=#241B4D/><ellipse cx=122 cy=110 rx=7 ry=6 fill=#241B4D/>
          <path d="M69 101 L87 103 M113 103 L131 101" stroke=#241B4D w3 round fill=none/>
idle:     stroke #241B4D w3.5 round: <path d="M70 106 Q78 112 86 106"/><path d="M114 106 Q122 112 130 106"/>
error:    stroke #241B4D w4 round join-round: <path d="M71 100 L85 107 L71 114"/><path d="M129 100 L115 107 L129 114"/>

Miệng (màu #B4445E):
idle, working: <path d="M93 130 Q100 136 107 130" stroke w3 round fill=none/>
done:          <path d="M91 128 L109 128 Q107 141 100 141 Q93 141 91 128 Z" fill/>
approval:      <ellipse cx=100 cy=132 rx=4.5 ry=5.5 fill/>
error:         <path d="M90 133 Q95 128 100 133 Q105 138 110 133" stroke w3 round fill=none/>

Phụ kiện theo mood:
approval: <path d="M144 192 Q160 172 164 148" stroke=O w16 round fill=none/><circle cx=165 cy=140 r=11 fill=#FFE6D8/>
          <circle cx=178 cy=42 r=16 fill=#FFB020/><path d="M178 32 L178 45" stroke=#241B4D w4 round/><circle cx=178 cy=52 r=2.5 fill=#241B4D/>
done:     fill #FFB020: <path d="M22 50 L26 60 L36 64 L26 68 L22 78 L18 68 L8 64 L18 60 Z"/>
          <path d="M178 28 L181 36 L189 39 L181 42 L178 50 L175 42 L167 39 L175 36 Z"/>
working:  <rect x=146 y=18 width=46 height=26 rx=13 fill=#fff/> + 3 chấm r=3.5 fill=O tại (159,31) (169,31) (179,31)
idle:     <path d="M160 30 L174 30 L160 44 L174 44" stroke=#C3C8D4 w3 round join-round fill=none/>   (chữ Z ngủ)
error:    <path d="M152 68 Q159 79 152 86 Q145 79 152 68 Z" fill=#7CC4F2/>                          (giọt mồ hôi)
```

## Phụ lục B — Bảng màu và kiểu chữ UI (từ mockup)

| Token | Giá trị | Dùng cho |
|---|---|---|
| bg | `#111318` | nền |
| surface | `#1A1D24` | thẻ |
| raised | `#222631` | chip |
| border | `#2C303B` | viền |
| text | `#E8EAF0` | chữ chính |
| muted | `#A3A9B8` | chữ phụ (đạt 4,5:1 trên surface) |
| approval | `#FFB020` | giơ tay, viền yêu cầu |
| working | `#8B7CF6` | đang làm |
| allow-bg / allow-fg | `#34D1B5` / `#06201B` | nút Đồng ý |
| risk-low | bg `#123A33` fg `#7EE6CF` | |
| risk-medium | bg `#3A2A0E` fg `#FFD27A` | |
| risk-high/critical | bg `#3A1A16` fg `#FF9F87` | |

Phông: Be Vietnam Pro (giao diện), JetBrains Mono (lệnh). Đóng gói phông cục bộ trong app, không tải từ mạng.

Bảng màu nhân vật mặc định (persona không cấu hình), theo thứ tự hash:
`(#2E2A5C,#8B7CF6,#FF7A59) (#5A3A22,#2BB5A0,#FFD166) (#1F1F24,#4C8DF6,#9AA3B5) (#7A2E3B,#E2627F,#FFB020) (#244A3A,#F2A65A,#8B7CF6) (#3B3B3B,#6FCF97,#56CCF2)`
