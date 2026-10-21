---
name: hook-lab
description: Xác nhận bằng thực nghiệm một hành vi hook của Claude Code (ví dụ PermissionRequest có làm treo terminal không, subagent gửi trường gì), ghi fixture và cập nhật docs/HOOK_FINDINGS.md. Dùng khi thiết kế phụ thuộc vào hành vi Claude Code chưa được kiểm chứng.
argument-hint: "[mã câu hỏi Q1…Q10 hoặc mô tả câu hỏi]"
disable-model-invocation: true
---

Câu hỏi cần trả lời: `$ARGUMENTS` (nếu trống: lấy các câu hỏi chưa có kết quả trong `docs/HOOK_FINDINGS.md`, ưu tiên Q1–Q4).

1. Đọc `docs/HOOK_FINDINGS.md` và mục "Kiến thức đã kiểm chứng" trong `CLAUDE.md`.
2. Với mỗi câu hỏi, giao cho agent `hook-spike` (mỗi câu hỏi một lần gọi, có thể chạy song song nếu độc lập).
   Nêu rõ cho agent: câu hỏi, giả thuyết, cách quan sát kết quả, nơi lưu fixture.
3. Gom kết quả: cập nhật bảng câu hỏi (cột Kết quả, Fixture) và mục Nhật ký.
4. Nếu kết quả mâu thuẫn với CLAUDE.md hoặc ARCHITECTURE.md: đề xuất sửa cụ thể (đoạn cũ → đoạn mới), chờ người duyệt
   rồi mới sửa hai file đó.
5. Nếu kết quả làm thay đổi một task trong `.squad/TASKS.md` (ví dụ Q1 cho thấy terminal bị treo → T12 cần nút
   "trả về terminal"): thêm ghi chú vào task đó.

Báo cáo cuối: mỗi câu hỏi một dòng "Q? → trả lời · bằng chứng · hệ quả thiết kế".
