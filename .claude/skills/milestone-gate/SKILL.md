---
name: milestone-gate
description: Kiểm tra cổng chất lượng cho một mốc phát hành của Arcturus (v0, v1…): test, bảo mật luồng duyệt, ngân sách tài nguyên, chỉ số dogfood. Xuất báo cáo đạt/chưa đạt và việc còn thiếu.
argument-hint: "[v0 | v1 | v2]"
disable-model-invocation: true
allowed-tools: Bash(git log *) Bash(git status *)
---

Mốc: `$ARGUMENTS` (mặc định v0).

Lịch sử gần đây: !`git log --oneline -15`

Kiểm tra theo thứ tự, mỗi mục ghi ĐẠT / CHƯA ĐẠT kèm bằng chứng:

1. **Task**: mọi task của mốc trong `.squad/TASKS.md` ở trạng thái `done` (hoặc có lý do hoãn rõ ràng).
2. **Test**: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm -C ui test` xanh.
3. **Fail-safe**: có test chứng minh: hub tắt → Claude Code hỏi bình thường; timeout → trả `{}`; không đường nào trả allow
   khi lỗi. Tìm test bằng Grep, liệt kê tên test.
4. **Vùng khóa**: mọi mẫu trong danger list có test; không có trust grant hay batch rule nào bỏ qua được danger list.
5. **Bảo mật**: gọi agent `security-auditor` (dev-squad) rà luồng `/hooks/permission`, WebSocket, xác thực token,
   hiển thị lệnh gốc. Ghi phát hiện mức cao/nghiêm trọng.
6. **Ngân sách**: gọi agent `budget-guard`; lấy dòng lịch sử mới nhất, phải có ít nhất một lần đo trên máy yếu.
7. **Dogfood** (v0): đọc mục Dogfood và Nhật ký trong `.squad/TASKS.md` và kết quả T20: dùng hằng ngày ≥2 tuần,
   thời gian xử lý trung bình mỗi yêu cầu giảm ≥30%, 0 lần tự duyệt sai.
8. **Tài liệu**: CLAUDE.md, ARCHITECTURE.md, PROTOCOL.md khớp với code (schema, endpoint, thông điệp).

Kết luận: "Đạt mốc" hoặc "Chưa đạt" kèm danh sách việc cần làm, mỗi việc gợi ý thành một task mới cho `.squad/TASKS.md`
(chỉ đề xuất, không tự thêm).
