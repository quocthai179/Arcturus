# Kết quả thực nghiệm hook Claude Code

Ghi lại bằng chứng thật (phiên bản Claude Code, lệnh đã chạy, JSON nhận được). Không ghi suy đoán vào cột "Kết quả".
Mỗi câu hỏi kèm fixture trong `fixtures/hooks/` nếu có.

Phiên bản Claude Code đã thử: `claude --version` → ____

## Câu hỏi mở

| # | Câu hỏi | Vì sao quan trọng | Kết quả | Fixture |
|---|---|---|---|---|
| Q1 | Trong lúc HTTP hook `PermissionRequest` đang chờ, terminal có hiện hộp thoại duyệt không, hay treo? | Quyết định timeout và nút "trả về terminal" | | |
| Q2 | Trả `{}` (2xx) thì Claude Code có hiện hộp thoại bình thường ngay không? | Fail-safe | | |
| Q3 | Trường nào trong đầu vào `PermissionRequest` chứa lệnh, tool, cwd, session_id? | Chuẩn hóa schema | | |
| Q4 | Subagent có gửi `PermissionRequest` riêng không? Có trường agent_id/agent_type không? | Nhân vật "thực tập sinh" | | |
| Q5 | Phiên nền (`claude --bg`, Agent View) có chạy HTTP hook như phiên thường không? | Đa phiên | | |
| Q6 | Trả `deny` kèm `message` thì Claude nhận lý do ở đâu, có tự đổi hướng không? | UX từ chối | | |
| Q7 | Nhiều phiên cùng gọi hook một lúc: có giới hạn đồng thời phía Claude Code không? | Hub phải chịu tải | | |
| Q8 | `claude agents --json` trả những trường nào, trạng thái `waitingFor` khớp với PermissionRequest không? | Đối soát | | |
| Q9 | Agent teams (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`): `TeammateIdle`, `TaskCreated`, `TaskCompleted` có dữ liệu gì? | Bảng task đội | | |
| Q10 | Hook `Notification` bắn khi nào, trùng với PermissionRequest không? | Tránh báo trùng | | |

## Đã biết từ tài liệu (cần xác nhận lại trên máy)

- HTTP hook: non-2xx, lỗi kết nối, hết giờ là lỗi không chặn; luồng bình thường tiếp tục.
- `PermissionRequest`: exit code 2 không có tác dụng; quyết định nằm trong `hookSpecificOutput.decision.behavior`.
- Hook allow không vượt được deny rule trong settings.
- Timeout mặc định của http hook: 600 giây.

## Nhật ký thử nghiệm

<!-- YYYY-MM-DD · câu hỏi · cách thử · kết quả · file fixture -->
