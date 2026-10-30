# Giao thức WebSocket hub ↔ UI

Nguồn sự thật cho kiểu: `crates/hub-core/src/protocol.rs` (`docs/PLAN.md` §3.4). Tài liệu này dành cho làn `ui` (T11–T14)
và mọi client khác. Đổi giao thức phải cập nhật file này cùng commit.

## Kết nối

```
GET ws://127.0.0.1:<cổng>/ws
Authorization: Bearer <token UI>        # hoặc ?token=<token UI> khi không đặt được header (webview)
```

- **Token UI** khác token của hook (`ARCTURUS_TOKEN`). Hub sinh token UI mới mỗi lần khởi động. Khi app tự khởi động hub
  (`--ui-token-stdout`, T12), token chỉ đi qua pipe tới app rồi tới webview qua `invoke('hub_info')`, không có file. Hub
  chạy riêng thì ghi `<data_dir>/arcturus/ui-token` (Windows: `%APPDATA%\arcturus\ui-token`; đổi bằng
  `--ui-token-file`) và app đọc file này (kết nối lại sau khi hub khởi động lại thì phải đọc lại). Token hook bị từ chối
  ở `/ws` (`401`).
- `Host` phải là `127.0.0.1:<cổng>`, `localhost:<cổng>` hoặc `[::1]:<cổng>`. `Origin`, nếu có, phải là một trong
  `http://tauri.localhost`, `https://tauri.localhost`, `tauri://localhost`; Vite dev (`http://127.0.0.1:5173`,
  `http://localhost:5173`) chỉ được nhận khi hub là bản debug. Sai → `403`.
- Tối đa 8 kết nối cùng lúc; vượt → `503`.
- Không đưa token vào log, URL hiển thị hay thông báo lỗi.

## Vòng đời

1. Sau khi nâng cấp, thông điệp **đầu tiên** luôn là `snapshot`: toàn bộ trạng thái hiện tại.
2. Sau đó là **delta**. Client áp delta theo kiểu **upsert, không giả định thứ tự tuyệt đối**:
   - `agent_updated`, `approval_requested`, `approval_handed_off`: thêm hoặc thay theo id.
   - `agent_removed`, `approval_resolved`, `handoff_cleared` cho id không biết: bỏ qua.
   - Có thể nhận trùng một thay đổi ngay sau `snapshot` (hub đăng ký kênh trước khi dựng snapshot để không lỡ).
3. Nhận `snapshot` ở **bất kỳ lúc nào** (client bị tụt sau kênh): **thay toàn bộ** trạng thái bằng snapshot đó.
4. Hub gửi Ping mỗi 30 giây; không trả Pong 2 lần liên tiếp thì hub đóng kết nối. Trình duyệt và tungstenite tự trả Pong.
5. Client không nhận được frame trong 10 giây (treo, mạng nghẽn) bị ngắt. Kết nối lại thì nhận snapshot mới.
6. Hub tắt: gửi Close `1001` ("hub tắt"). Mọi yêu cầu đang chờ đã được trả `{}` (Claude Code hỏi trong terminal).

Giới hạn: frame từ client tối đa 64 KB; `decide`, `team_pause`, `team_resume`, `focus_set` chung tối đa 20 lần/giây mỗi
kết nối; `message` khi từ chối tối đa 500 ký tự.

## Server → client

### `snapshot`

```json
{"t":"snapshot",
 "agents":[ AgentView… ],
 "queue":[ ApprovalView… ],
 "handoffs":[ HandoffView… ],
 "team":{"paused":false,"focus":false,"roster_max":5}}
```

### `agent_updated` / `agent_removed`

```json
{"t":"agent_updated","agent":{
  "id":"agt_cd99a479-…","parent_id":null,"kind":"session",
  "name":"Khoa","role":"backend","palette":{"hair":"#1F1F24","outfit":"#4C8DF6","acc":"#9AA3B5"},
  "mood":"working","activity":"typing","act_text":"Đang sửa LoginForm.tsx",
  "host":"laptop","source":"claude-code","cwd":"D:\\code\\Arcturus\\.claude\\worktrees\\hub",
  "context_pct":null,"last_event_at":"2026-10-22T09:00:00Z"}}
{"t":"agent_removed","id":"agt_cd99a479-…"}
```

- `kind`: `session` | `subagent` | `teammate`; agent con có `parent_id`.
- `mood`: `idle` | `working` | `approval` | `error` | `done`. `activity`: `typing` | `reading` | `running` | `thinking` |
  `waiting` | `idle`.
- `act_text` chỉ để hiển thị (đã bỏ ký tự điều khiển, lệnh Bash cắt 60 ký tự). `"Duyệt trong terminal"` nghĩa là agent đang
  chờ người duyệt trong terminal (xem `approval_handed_off`).
- Thay đổi chỉ `last_event_at` không được gửi; giá trị mới nhất có trong snapshot.

### `approval_requested`

```json
{"t":"approval_requested","approval":{
  "request_id":"req_01M5…","agent_id":"agt_s1","agent_name":"Khoa","tool":"Bash",
  "command_original":"git push origin main","command_normalized":"git push origin main",
  "tool_options":{"run_in_background":"true"},
  "danger_spans":[],"suspicions":[],"truncated":false,
  "risk":"medium","reversible":false,"group_key":"Bash",
  "created_at":"2026-10-22T09:00:00Z","deadline_at":"2026-10-22T09:01:50Z","countdown_ms":null}}
```

Quy tắc hiển thị thẻ duyệt (quy tắc bất biến 3):
- **Cơ sở duyệt là `command_normalized`**, không phải tóm tắt nào khác. Render bằng text node, **không bao giờ** như HTML,
  không cắt bằng CSS (ellipsis, overflow ẩn). Mọi chuỗi từ agent (`command_*`, `tool_options`, `cwd`, `act_text`, `tool`,
  `agent_name`) đều không tin cậy.
- `tool_options` (Bash): các trường ngoài `command` đổi cách lệnh chạy, ví dụ `dangerouslyDisableSandbox: true` (chạy ngoài
  sandbox), `run_in_background`, `timeout`. Hiện **rõ, tách khỏi lệnh**, nổi bật khi `dangerouslyDisableSandbox` là `true`.
- Ký tự ẩn/điều khiển/bidi/khoảng trắng lạ đã được thay bằng `⟨U+XXXX⟩` (chuỗi lặp `⟨U+XXXX×N⟩`); có
  `"hidden_chars"` trong `suspicions` thì hiện cảnh báo rõ ràng.
- `danger_spans`: `[start, end)` theo **chỉ số ký tự Unicode** trong `command_normalized`. Cắt bằng
  `Array.from(s).slice(start, end).join('')`, không dùng `String.prototype.slice`. Mỗi khoảng là chỗ khớp một luật vùng
  khóa (T8); có thể rỗng kể cả khi `risk` là `critical` (chỗ khớp nằm trong nội dung đã cắt, hoặc luật không có vị trí).
  Ví dụ dùng cho test tô sáng: `echo "🚀 thư mục" && rm -rf đầu-ra` → `[[20,33]]` (`rm -rf đầu-ra`; theo byte UTF-8 là 26, theo UTF-16 là 21).
- `risk` do hub tính (T8), không tin agent: `critical` là vùng khóa (luôn hỏi người, không bao giờ tự duyệt, không đếm
  ngược), `high` có cờ nghi ngờ hoặc không phân tích được, `low` chỉ đọc/test/build trong workspace. `reversible`
  hiện chỉ `true` khi `risk` là `low`.
- `suspicions` (thứ tự cố định): `hidden_chars` (ký tự ẩn), `subshell` (`$(…)`, `` `…` ``, `<(…)`), `eval` (`eval`,
  `source`, `.`, `iex`), `encoded` (`base64 -d`, `xxd -r`, `printf '\x…'`…). Có cờ nào thì `risk` ít nhất là `high`.
- `truncated: true` (chỉ Edit/Write/MultiEdit/NotebookEdit): nội dung trên thẻ đã bị cắt. UI bắt người dùng mở bản đầy đủ
  (`GET /approvals/{request_id}/input`) trước khi cho bấm Allow; hub cũng giữ nguyên mọi `allow` cho thẻ này tới khi bản
  đầy đủ đã được lấy (trả `feed` "Cần mở bản đầy đủ trước khi duyệt"). `deny` không cần.
- Sàn thời gian duyệt: hub giữ mọi `allow` tới sớm hơn `min_review_ms` kể từ lúc hub phát `approval_requested` (`feed`
  "Duyệt quá nhanh: xem lại thẻ rồi bấm lại"). Thẻ tới muộn qua `snapshot` không được sàn của hub bảo vệ, nên UI làm mờ
  nút Allow trong `min_review_ms` kể từ lúc **hiện** thẻ. Đếm ngược ngắn hơn sàn thì UI chờ tới hết sàn mới gửi.
- `countdown_ms` khác `null`: đếm ngược phủ quyết; hết giờ thì UI gửi `decide{decision:"allow",origin:"countdown"}`. Khi
  `team.paused` là `true`, UI dừng **mọi** đếm ngược (hub cũng từ chối đếm ngược khi tạm dừng).

### `approval_resolved`

```json
{"t":"approval_resolved","request_id":"req_01M5…","decision":"allow","decided_by":"user"}
{"t":"approval_resolved","request_id":"req_01M5…","decision":null,"decided_by":"timeout"}
```

`decided_by`: `user` | `countdown` | `policy` | `timeout` | `cancelled`. `decision` là `null` khi `timeout`/`cancelled` (hub
đã trả `{}`, Claude Code hỏi trong terminal). Bỏ thẻ khỏi hàng đợi.

### `approval_handed_off` / `handoff_cleared`

Lệnh quá dài để duyệt trên thẻ (mặc định > 4.096 byte UTF-8 hoặc > 50 dòng; không áp dụng cho Edit/Write/MultiEdit/
NotebookEdit). Hub **đã trả `{}`**: người duyệt trong hộp thoại của terminal, nơi lệnh hiện đầy đủ.

```json
{"t":"approval_handed_off","handoff":{
  "handoff_id":"hof_01M5…","agent_id":"agt_s1","agent_name":"Khoa","tool":"Bash",
  "bytes":18230,"lines":412,
  "preview_head":"#!/bin/sh\nset -e\n…","preview_tail":"…\necho done",
  "suspicions":["hidden_chars"],
  "message":"Lệnh quá dài để duyệt ở đây — hãy mở terminal của Khoa để xem đủ và quyết định.",
  "created_at":"2026-10-22T09:00:00Z"}}
{"t":"handoff_cleared","handoff_id":"hof_01M5…"}
```

- Thẻ này **không có nút Allow/Deny**, không đếm ngược. Gửi `decide` với `handoff_id` chỉ nhận `feed` từ chối.
- `suspicions` tính trên lệnh **đầy đủ**, kể cả phần không có trong xem trước.
- `handoff_cleared` khi agent đã sang bước khác (đã được duyệt hay từ chối trong terminal) hoặc phiên kết thúc.

### `team_updated`

```json
{"t":"team_updated","team":{"paused":true,"focus":false,"roster_max":5}}
```

### `feed`

```json
{"t":"feed","text":"Yêu cầu đã hết hạn","ts":"2026-10-22T09:00:00Z"}
```

Tin nhắn cho người dùng, thường **chỉ gửi cho client vừa gửi lệnh**:

| `text` | Khi nào |
|---|---|
| `Yêu cầu đã hết hạn` | `decide` cho yêu cầu không còn chờ (đã hết giờ, đã quyết, đã hủy) |
| `Cần trưởng nhóm bấm duyệt` | `decide` với `origin: countdown` không hợp lệ (không có đếm ngược, chưa hết giờ, đang tạm dừng, hoặc kèm `deny`); yêu cầu vẫn chờ |
| `Cần mở bản đầy đủ trước khi duyệt` | `allow` cho thẻ `truncated` khi chưa lấy `GET /approvals/{id}/input`; yêu cầu vẫn chờ |
| `Duyệt quá nhanh: xem lại thẻ rồi bấm lại` | `allow` (`origin: user` hoặc `countdown`) tới sớm hơn `min_review_ms` (mặc định 1.000 ms) kể từ lúc hub phát `approval_requested`; yêu cầu vẫn chờ, bấm lại sau là được. `deny` không bị chặn |
| `Yêu cầu này đang được duyệt trong terminal` | `decide` cho `handoff_id` |
| `Lý do từ chối quá dài (tối đa 500 ký tự)` | `message` quá dài; yêu cầu vẫn chờ |
| `Quá nhiều lệnh duyệt, hãy thử lại sau` | quá 20 `decide`/giây |
| `Quá nhiều lệnh, hãy thử lại sau` | quá 20 lệnh/giây tính cả `team_pause`, `team_resume`, `focus_set` |
| `Đã duyệt N yêu cầu` / `Đã từ chối N yêu cầu` (+ `, M yêu cầu chưa đủ điều kiện (xem kỹ rồi bấm lại)`, + `, K yêu cầu vùng khóa ở lại, duyệt từng cái`) | kết quả `decide_group` |
| `Không có yêu cầu nào trong nhóm này` | `decide_group` không khớp yêu cầu nào |
| `Thời hạn phải từ 1 tới 240 phút` / `Nhóm không hợp lệ` / `Lệnh ghép (…) không được cho phép theo nhóm` / `Đã mở quá nhiều quy tắc, hãy tắt bớt` | `batch_open` bị từ chối |
| `Không có quy tắc nào đang mở cho nhóm này` | `batch_close` không có gì để đóng |
| `Quá nhiều lệnh duyệt, hãy thử lại sau` | quá 20 lệnh/giây tính cả `decide_group`, `batch_open`, `batch_close` |
| `Sẽ không đề xuất lại trong 7 ngày` | `suggestion_answer` với `accept: false` |
| `Đề xuất không còn hiệu lực` | `suggestion_answer` cho id không biết (đã trả lời, hoặc chuỗi đã đứt vì bị từ chối) |
| `Đã cho phép <persona> tự chạy "<nhóm>" trong N ngày (chỉ rủi ro thấp/vừa)` | broadcast khi `suggestion_answer` chấp nhận |
| `<persona>: hạ quyền tự chạy "<nhóm>" xuống mức 5 (hỏi kèm đếm ngược) vì bạn từ chối` / `<persona>: đã thu hồi quyền tự chạy "<nhóm>" vì bạn từ chối` | broadcast khi người từ chối một yêu cầu thuộc nhóm đang có grant |
| `Thông điệp không hợp lệ` | JSON sai hoặc `t` không biết |
| `Chỉ nhận thông điệp JSON dạng text` | frame nhị phân |

Thông điệp sai không làm đóng kết nối.

### `suggestion` (T10)

```json
{"t":"suggestion","suggestion":{"id":"sug_01M5…","subject":"Khoa","pattern":"Bash:npm test","level":6,
 "approved_in_row":20,"window_days":7,"grant_days":30,"created_at":"2026-10-27T10:00:00Z",
 "text":"Khoa đã được bạn duyệt 20 lần liên tiếp cho \"Bash:npm test\" trong 7 ngày. Cho phép tự chạy trong 30 ngày? (Chỉ yêu cầu rủi ro thấp/vừa; vùng khóa vẫn hỏi bạn.)"}}
```

- Hub gửi khi chuỗi duyệt của **người** (không tính đếm ngược, policy, batch) cho một cặp (persona, `group_key`) đạt
  `[trust] streak_threshold` lần liên tiếp trong `streak_window_days` ngày, không có sự cố (người từ chối) trong cửa sổ đó,
  và cặp đó chưa có grant mức 6. Chỉ yêu cầu risk ≤ Medium và lệnh không ghép mới được đếm. Một lần cho mỗi cặp cho tới khi
  được trả lời.
- Client mới kết nối (và client vừa tụt kênh nhận `Snapshot` mới) nhận lại các đề xuất đang chờ **ngay sau `Snapshot`**.
- `subject`, `pattern`, `text` là dữ liệu không tin cậy (tên persona và tên lệnh): ký tự ẩn đã hiện `⟨U+XXXX⟩`; hiển thị như
  văn bản thuần.
- Trả lời bằng `suggestion_answer {id, accept}`. Chấp nhận → grant mức 6 trong `grant_days` ngày (tự duyệt yêu cầu risk ≤ Medium
  của đúng cặp đó, `decided_by: policy`, lý do `trust:<id>`). Từ chối → không đề xuất lại cặp đó trong 7 ngày (chỉ giữ trong
  bộ nhớ hub, mất khi hub khởi động lại). Người từ chối một yêu cầu của nhóm đang có grant → hạ 6 → 5 (hỏi người kèm đếm ngược 10
  giây, chỉ cho yêu cầu `reversible`), từ chối nữa → thu hồi; mọi thay đổi ghi `trust_log`.
- Hub không có thông điệp "đề xuất đã được trả lời nơi khác": client thấy lại đề xuất cho tới khi tải lại; trả lời lần hai nhận
  `Đề xuất không còn hiệu lực`.

## Client → server

```json
{"t":"decide","request_id":"req_01M5…","decision":"allow","origin":"user"}
{"t":"decide","request_id":"req_01M5…","decision":"deny","origin":"user","message":"dùng cargo clean"}
{"t":"decide","request_id":"req_01M5…","decision":"allow","origin":"countdown"}
{"t":"team_pause"}
{"t":"team_resume"}
{"t":"focus_set","on":true}
{"t":"decide_group","group_key":"Bash:npm test","decision":"allow"}   // T9
{"t":"batch_open","group_key":"Bash:npm test","minutes":60}           // T9
{"t":"batch_close","group_key":"Bash:npm test"}                       // T9
{"t":"suggestion_answer","id":"…","accept":true}                // T10
```

- `group_key` (`ApprovalView.group_key`): Bash/PowerShell: `<tool>:<[lệnh bọc ]chương trình[ lệnh con]>` (bỏ đường dẫn, chữ
  thường), ví dụ `Bash:npm test`, `Bash:cargo test`, `Bash:git push`, `Bash:sudo apt update`; tool khác `<tool>:*`. Lệnh
  **ghép** (nhiều lệnh, có `$(…)`, cờ nghi ngờ, không phân tích được) mang hậu tố ` (ghép)` và chỉ gom với nhau.
- `decide_group`: áp `decision` cho mọi yêu cầu đang chờ cùng `group_key` **trừ Critical** (ở lại hàng đợi). Mỗi yêu cầu qua
  đúng các điều kiện của `decide` với `origin: user` (sàn `min_review_ms`, thẻ cắt phải mở bản đầy đủ, đội tạm dừng); yêu cầu
  chưa đủ điều kiện được giữ nguyên, không bị từ chối. Kết quả từng yêu cầu đến qua `approval_resolved`; client gửi lệnh nhận
  thêm một `feed` tóm tắt. Không có `origin: countdown` cho duyệt gộp.
- `batch_open {group_key, minutes}`: trong `minutes` (1–240) phút, yêu cầu **trùng khóa** và có risk ≤ Medium được hub tự duyệt
  (`decided_by: policy`, lý do `batch:<group_key>`); High và Critical không bao giờ; khóa ghép bị từ chối; đội đang tạm
  dừng thì không tự duyệt gì. Mở lại cùng khóa là gia hạn (tính từ lúc đó). Tối đa 64 quy tắc. Quy tắc được lưu (bảng
  `batch_rules`) và nạp lại khi hub khởi động; hết hạn thì hub broadcast `feed` "Hết thời hạn cho phép …" (một timer duy nhất
  cho mỗi quy tắc, không polling). `batch_open` thành công và `batch_close` đều broadcast `feed` cho mọi client. Snapshot
  chưa có danh sách quy tắc đang mở (cần đổi hợp đồng, chưa làm).
- Kết quả của `decide` thành công đến qua `approval_resolved` (broadcast cho mọi client), không có trả lời riêng.
- `team_pause` xóa đếm ngược của mọi yêu cầu đang chờ (snapshot sau đó có `countdown_ms: null`); `team_resume` không
  khôi phục. Lệnh của cùng một kết nối được xử lý đúng thứ tự gửi.
- Không có `return_to_terminal`: terminal luôn hỏi song song (Q1), và hub tự trả lệnh quá dài về terminal.

## HTTP cho hook (Claude Code → hub)

`POST /hooks/event` (trả `204`, xử lý sau) và `POST /hooks/permission` (giữ tới khi có quyết định hoặc hết giờ, trả `200` + JSON).
Cả hai cần `Authorization: Bearer <token hook>` (`ARCTURUS_TOKEN` của hub; app tự sinh nếu chưa đặt, xem `docs/DOGFOOD.md` mục 0); token UI không được chấp nhận ở đây.

- `Host` phải là loopback của chính hub (`127.0.0.1:<cổng>`, `localhost:<cổng>`, `[::1]:<cổng>`), nếu không `403` (chặn DNS rebinding;
  cùng luật với `/ws`). Token hook nằm trong môi trường của agent nên không coi là bí mật với agent.
- Body tối đa 1 MiB. `/hooks/permission` đặt chỗ trong ngân sách byte của hàng đợi duyệt trước khi đọc body (hết chỗ thì `200 {}`).
  `/hooks/event` có ngân sách byte riêng cho hàng đợi chờ xử lý (8 MiB ước lượng sau parse): vượt thì **bỏ sự kiện** (vẫn `204`,
  có log); không bao giờ ảnh hưởng luồng duyệt.
- `GET /healthz` → `200 ok`, không cần token, không có dữ liệu (dùng để kiểm hub sống).

## HTTP cho UI

```
GET /approvals/{request_id}/input
Authorization: Bearer <token UI>        # chỉ header, không nhận ?token=
```

- `200`, `Content-Type: application/json`, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`: `tool_input`
  đầy đủ (JSON gốc của hook) của yêu cầu đang chờ. Hiển thị như văn bản thuần. Lấy thành công thì hub ghi nhận "đã mở bản
  đầy đủ" cho yêu cầu đó (mở khóa allow khi `truncated`).
- Route không có CORS. Webview gọi `fetch` kèm header `Authorization` sẽ bị preflight chặn: app gọi qua phía Rust của
  Tauri (T12), không thêm CORS dễ dãi.
- `404`: không còn chờ, không tồn tại, hoặc là `handoff_id`. `401`: thiếu/sai token UI. `403`: `Host`/`Origin` sai.

```
GET /standup?hours=24
Authorization: Bearer <token UI>        # chỉ header; hours 1..=168, mặc định 24
```

- `200`, `application/json`, `nosniff`, `no-store` (có tiêu đề commit và lệnh test của người dùng): standup của `hours` giờ gần
  nhất, **không LLM** (quy tắc bất biến 5). Số liệu đếm từ sự kiện đã lưu trong SQLite, `git log` chạy trong các `cwd` của
  persona (tối đa 5 thư mục, 3 giây mỗi thư mục, bỏ merge), văn bản sinh từ template tiếng Việt cố định.
- Hình dạng: `{"since","until","text","personas":[{"name","turns_done","commits":[…≤5],"commit_count","tests_passed",
  "tests_failed","pending","repeated_errors":[{"tool","what","count"}]}]}`. `repeated_errors`: cùng một lỗi công cụ ≥3 lần.
  `tests_*`: lệnh Bash khớp bộ chạy test phổ biến (`PostToolUse` = đạt, `PostToolUseFailure` = lỗi). `pending`: yêu cầu duyệt
  đang chờ lúc gọi.
- Mọi chuỗi (tên, lệnh, tiêu đề commit) là dữ liệu không tin cậy: ký tự ẩn hiện thành `⟨U+XXXX⟩`, gộp khoảng trắng, cắt 120
  ký tự. UI hiển thị như văn bản thuần.
- `400`: `hours` ngoài khoảng hoặc không phải số. `401`/`403` như các route UI khác. `503`: hub chạy không có cơ sở dữ liệu.
