# Kết quả thực nghiệm hook Claude Code

Ghi lại bằng chứng thật (phiên bản Claude Code, lệnh đã chạy, JSON nhận được). Không ghi suy đoán vào cột "Kết quả".
Mỗi câu hỏi kèm fixture trong `fixtures/hooks/` nếu có.

Phiên bản Claude Code đã thử: `claude --version` → `2.1.239` (phiên headless 1–5), `2.1.306` (phiên tương tác Q1/Q2 và
lần kiểm lại SessionStart; CLI tự cập nhật giữa chừng), `2.1.308` (Q1b ở T3, hub luồng duyệt thật). Windows 11, hook trỏ
về hub ở `127.0.0.1:7879`.

## Câu hỏi mở

| # | Câu hỏi | Vì sao quan trọng | Kết quả | Fixture |
|---|---|---|---|---|
| Q1 | Trong lúc HTTP hook `PermissionRequest` đang chờ, terminal có hiện hộp thoại duyệt không, hay treo? | Quyết định timeout và nút "trả về terminal" | **Không treo: hộp thoại hiện song song** trong lúc hub còn giữ request (v2.1.306, `--record-delay 20`). Hub nhận lúc 16:29:20.5; màn hình đọc trước 16:29:26 đã có "Do you want to proceed? 1. Yes … 4. No"; `Notification` `permission_prompt` tới lúc 16:29:26.6. Người dùng có thể trả lời ở terminal **trước** khi hub trả lời (xem Q1b). | `fixtures/hooks/PermissionRequest/01M5FMQE8JCRSZ8PXH45HV306F.json`, `fixtures/hooks/Notification/01M5FMQM4X9CTXY61KY2MW7M16.json` |
| Q1b | Người trả lời ở terminal trong lúc hub còn giữ request: Claude Code có đóng kết nối HTTP không? | Dọn yêu cầu đã được trả lời ở nơi khác (T3) | **Không đóng** (v2.1.308). Chỉ bật hook `PermissionRequest`: hub nhận 05:09:09.2; bấm "1. Yes" ở terminal 05:09:12.2; lệnh chạy ngay (`q1b-marker.txt` tạo lúc 05:09:13.27) trong khi hub **vẫn giữ** kết nối; kết nối chỉ đóng lúc 05:09:31.26, ngay sau khi tiến trình `claude` bị tắt (≈18 s sau khi lệnh chạy). Câu trả lời muộn của hub sau đó không còn tác dụng. Hệ quả: hub phải tự nhận ra yêu cầu đã được trả lời: T3 hủy (trả `{}`) khi tới `PostToolUse`/`PostToolUseFailure` cùng agent, cùng `tool_name` + `tool_input` (lần chạy đủ hook: hủy 1,2 s sau khi bấm), hoặc `Stop`/`StopFailure`/`UserPromptSubmit`/`SubagentStop` của cùng agent; tắt tiến trình thì guard ngắt kết nối hủy. | Log hub trong Nhật ký (không có fixture mới: body `PermissionRequest` giống Q1) |
| Q2 | Trả `{}` (2xx) thì Claude Code có hiện hộp thoại bình thường ngay không? | Fail-safe | **Có.** Tương tác: hộp thoại đã hiện từ trước và giữ nguyên sau khi hub trả `{}` (16:29:40), vẫn chờ người trả lời. Headless `-p`: `{}` → không có người để hỏi → Claude Code **từ chối** lệnh (`marker-p.txt` không được tạo) và giải thích. Không trường hợp nào `{}` thành allow. | `fixtures/hooks/PermissionRequest/01M5FKGN3X96RXWT3QPR7887AR.json` |
| Q3 | Trường nào trong đầu vào `PermissionRequest` chứa lệnh, tool, cwd, session_id? | Chuẩn hóa schema | Cấp cao nhất: `session_id`, `cwd`, `transcript_path`, `scratchpad_dir`, `prompt_id`, `permission_mode`, `effort.level`, `hook_event_name`, `tool_name`, `tool_input` (Bash: `command`, `description`), `permission_suggestions` (mảng `{type: "addDirectories" \| "setMode", …, destination}`). **Không có `tool_use_id`** (PreToolUse/PostToolUse có). Subagent thêm `agent_id`, `agent_type`. | `fixtures/hooks/PermissionRequest/01M5FKGN3X96RXWT3QPR7887AR.json` |
| Q4 | Subagent có gửi `PermissionRequest` riêng không? Có trường agent_id/agent_type không? | Nhân vật "thực tập sinh" | **Có.** Subagent `general-purpose` chạy `touch sub-marker.txt` → `PermissionRequest` riêng với `agent_id` (ví dụ `ad7e4b71226dc3e83`) và `agent_type: "general-purpose"`, cùng `session_id` với phiên cha. `SubagentStart`/`SubagentStop` có `agent_id`, `agent_type` (`SubagentStop` thêm `agent_transcript_path`); mọi PreToolUse/PostToolUse của subagent mang `agent_id`. | `fixtures/hooks/PermissionRequest/01M5FKHVNVJ4DXZ5JJ78SM8SV9.json`, `fixtures/hooks/SubagentStart/01M5FKF3AQKEB0Z0XC6Z6Q9VDZ.json`, `fixtures/hooks/PreToolUse/01M5FKF68K2YRM92H05BZDE4XM.json` |
| Q5 | Phiên nền (`claude --bg`, Agent View) có chạy HTTP hook như phiên thường không? | Đa phiên | | |
| Q6 | Trả `deny` kèm `message` thì Claude nhận lý do ở đâu, có tự đổi hướng không? | UX từ chối | | |
| Q7 | Nhiều phiên cùng gọi hook một lúc: có giới hạn đồng thời phía Claude Code không? | Hub phải chịu tải | | |
| Q8 | `claude agents --json` trả những trường nào, trạng thái `waitingFor` khớp với PermissionRequest không? | Đối soát | | |
| Q9 | Agent teams (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`): `TeammateIdle`, `TaskCreated`, `TaskCompleted` có dữ liệu gì? | Bảng task đội | | |
| Q10 | Hook `Notification` bắn khi nào, trùng với PermissionRequest không? | Tránh báo trùng | *Một phần, chưa kết luận:* ở phiên tương tác, `Notification` `{notification_type: "permission_prompt", message: "Claude needs your permission"}` tới ~6 s sau `PermissionRequest` của cùng yêu cầu, không có `tool_name`. Chưa thử các loại notification khác. | `fixtures/hooks/Notification/01M5FMQM4X9CTXY61KY2MW7M16.json` |

## Phát hiện thêm (ngoài Q1–Q10)

- **F1 · `SessionStart` không chạy hook `type: "http"`.** Trên cả v2.1.239 và v2.1.306: hook http cho `SessionStart`
  trong `--settings` không gửi gì, còn hook `type: "command"` đặt cùng chỗ thì chạy. Đường vòng đã thử được: hook command
  `curl -s -m 3 -X POST -H "Authorization: Bearer $ARCTURUS_TOKEN" -H "Content-Type: application/json" --data-binary @- http://127.0.0.1:7879/hooks/event`
  (stdin là JSON hook) → hub nhận `{session_id, transcript_path, cwd, hook_event_name: "SessionStart", source: "startup"}`.
  Fixture: `fixtures/hooks/SessionStart/01M5FMS4NG4F1NQTK82432ZW92.json`.
- **F2 · Chế độ quyền quyết định có `PermissionRequest` hay không.** Phiên tương tác ở `auto mode` chạy `touch` mà không
  hỏi và không gửi `PermissionRequest`. Lệnh chỉ đọc như `hostname` chạy không hỏi cả ở chế độ `default`.
- **F3 · Headless `-p` vẫn gửi `PermissionRequest`** khi lệnh cần hỏi; hub trả `{}` thì lệnh bị từ chối (Q2).
- **F4 · `StopFailure`** có `error` (ví dụ `"authentication_failed"`) và `last_assistant_message`; `SessionEnd` có
  `reason` (`"other"` ở headless). Fixture: `fixtures/hooks/StopFailure/01M5FJ615WHQFWK62S9TFPSK1E.json`.
- **F5 · Trường chung của gần như mọi sự kiện:** `session_id`, `transcript_path`, `cwd`, `prompt_id`, `hook_event_name`;
  thường có thêm `scratchpad_dir`, `permission_mode`, `effort.level`. Trên Windows `cwd` và đường dẫn dùng `\`, khớp quyết
  định chuẩn hóa `match_cwd` ở `docs/PLAN.md` §3.7.
- **F6 · Bash tool thừa hưởng biến môi trường của tiến trình `claude`, kể cả `ARCTURUS_TOKEN`** (v2.1.308, T5). Headless
  trong thư mục sandbox, không hook: `ARCTURUS_TOKEN=<token giả> claude -p "... printenv ARCTURUS_TOKEN" --allowedTools
  "Bash(printenv ARCTURUS_TOKEN)"` → in đúng token giả. Hệ quả: token của hook đọc được bởi mọi agent (không cần người
  duyệt nếu lệnh đã allow sẵn). Vì vậy `/ws` và `/approvals/*` dùng token UI riêng, không nằm trong môi trường Claude Code
  (quyết định D1 ở T5, `docs/PLAN.md` §3.3).

## Đã biết từ tài liệu (cần xác nhận lại trên máy)

- HTTP hook: non-2xx, lỗi kết nối, hết giờ là lỗi không chặn; luồng bình thường tiếp tục. *(T1 mới xác nhận trường hợp
  2xx `{}` ở Q2; non-2xx, lỗi kết nối và hết giờ chưa thử trực tiếp, để T17.)*
- `PermissionRequest`: exit code 2 không có tác dụng; quyết định nằm trong `hookSpecificOutput.decision.behavior`.
- Hook allow không vượt được deny rule trong settings.
- Timeout mặc định của http hook: 600 giây.

## Nhật ký thử nghiệm

<!-- YYYY-MM-DD · câu hỏi · cách thử · kết quả · file fixture -->
- 2026-10-21 · chuẩn bị · `hub --record <scratchpad>/rec --bind 127.0.0.1:7879`; settings thí nghiệm
  `lab/settings.local.json` (mọi sự kiện → `/hooks/event`, `PermissionRequest` → `/hooks/permission`, timeout 120 s),
  nạp bằng `claude --settings lab/settings.local.json`; không đụng `~/.claude/settings.json`. Phiên headless chạy trong
  thư mục sandbox ngoài repo; phiên tương tác chạy trong `lab/`. Ghi 64 file, làm sạch bằng
  `replay scrub <rec> fixtures/hooks --home 'C:\Users\PC'` (home, slug thư mục dự án, token, tên máy), rà bằng grep.
- 2026-10-21 · Q3, F5 · headless: Write/Read/Edit/`echo`/`ls`/Glob/Grep với `--allowedTools` → đủ PreToolUse/PostToolUse.
- 2026-10-21 · Q4 · headless: Agent → subagent đọc file, chạy `echo`; lần sau subagent chạy `touch sub-marker.txt` không
  được phép → `PermissionRequest` có `agent_id`/`agent_type`.
- 2026-10-21 · Q2, F2, F3 · headless: `hostname` chạy không hỏi; `touch marker-p.txt` → `PermissionRequest` → `{}` → bị từ chối.
- 2026-10-21 · F1 · hook http cho SessionStart không bắn (2.1.239, kiểm lại trên 2.1.306); hook command + curl thì có.
- 2026-10-21 · Q1, Q2, Q10 · tương tác `claude --permission-mode default` trong tab Terminal, hub `--record-delay 20`,
  đọc màn hình ở +6 s và sau +20 s. Lần chạy đầu ở auto mode nên không có PermissionRequest (F2).
- 2026-10-22 · Q1b (T3) · hub luồng duyệt thật (`hub --bind 127.0.0.1:7879`, timeout 110 s), Claude Code 2.1.308
  tương tác `--permission-mode default` trong `lab/`, lái bằng ConPTY (`node-pty`): gửi prompt
  "run the command: touch q1b-marker.txt", chờ hộp thoại, bấm `1` sau 3 s. Ba lần chạy:
  1. Đủ hook, script không nhận ra hộp thoại nên không bấm: hub giữ 05:05:26.8 → hết giờ 05:07:16.8, trả `{}`; hộp thoại
     vẫn hiện, chờ người (khớp Q2 trên 2.1.308).
  2. Đủ hook (`lab/settings.local.json`), bấm Yes 05:08:20.1: lệnh chạy, `PostToolUse` tới, hub log
     `hủy yêu cầu duyệt, trả {} … why="tool đã chạy xong: yêu cầu được trả lời ở nơi khác"` lúc 05:08:21.29.
  3. Chỉ hook `PermissionRequest`: bấm Yes 05:09:12.2, file tạo 05:09:13.27, hub vẫn giữ; log
     `kết nối hook đóng trước khi có quyết định, hủy yêu cầu` lúc 05:09:31.26, ngay sau khi script tắt `claude`
     (05:09:30.2 + Ctrl-C). Kết luận: Claude Code không đóng kết nối khi người trả lời ở terminal; chỉ đóng khi tiến trình
     thoát (hoặc hết timeout của hook).
