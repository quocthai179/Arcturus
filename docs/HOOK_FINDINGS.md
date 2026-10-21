# Kết quả thực nghiệm hook Claude Code

Ghi lại bằng chứng thật (phiên bản Claude Code, lệnh đã chạy, JSON nhận được). Không ghi suy đoán vào cột "Kết quả".
Mỗi câu hỏi kèm fixture trong `fixtures/hooks/` nếu có.

Phiên bản Claude Code đã thử: `claude --version` → `2.1.239` (phiên headless 1–5), `2.1.306` (phiên tương tác Q1/Q2 và
lần kiểm lại SessionStart; CLI tự cập nhật giữa chừng). Windows 11, hook trỏ về `hub --record` ở `127.0.0.1:7879`.

## Câu hỏi mở

| # | Câu hỏi | Vì sao quan trọng | Kết quả | Fixture |
|---|---|---|---|---|
| Q1 | Trong lúc HTTP hook `PermissionRequest` đang chờ, terminal có hiện hộp thoại duyệt không, hay treo? | Quyết định timeout và nút "trả về terminal" | **Không treo: hộp thoại hiện song song** trong lúc hub còn giữ request (v2.1.306, `--record-delay 20`). Hub nhận lúc 16:29:20.5; màn hình đọc trước 16:29:26 đã có "Do you want to proceed? 1. Yes … 4. No"; `Notification` `permission_prompt` tới lúc 16:29:26.6. Người dùng có thể trả lời ở terminal **trước** khi hub trả lời (chưa thử điều gì xảy ra khi đó, xem Q1b trong Nhật ký). | `fixtures/hooks/PermissionRequest/01M5FMQE8JCRSZ8PXH45HV306F.json`, `fixtures/hooks/Notification/01M5FMQM4X9CTXY61KY2MW7M16.json` |
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
- Chưa thử (Q1b): người dùng trả lời ở terminal **trong lúc** hub còn giữ request thì Claude Code đóng kết nối HTTP hay
  để hub chờ tới hết giờ. Cần một người bấm phím trong 20 giây; đề xuất làm ở T3 hoặc T17.
