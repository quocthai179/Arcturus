---
name: budget-check
description: Đo RAM/CPU/độ trễ của hub và app Arcturus so với ngân sách trong docs/BUDGET.md và ghi lịch sử đo. Dùng sau khi sửa crates/hub, crates/hub-core hoặc crates/app, hoặc trước khi merge.
argument-hint: "[hub | app | all]"
disable-model-invocation: true
allowed-tools: Bash(git log *)
---

Phạm vi đo: `$ARGUMENTS` (mặc định: hub).

Commit gần đây (kèm file đã đổi):
!`git log --oneline --stat -3`

Giao cho agent `budget-guard` chạy kịch bản chuẩn trong `docs/BUDGET.md` cho phạm vi trên.
Sau đó trả lời ngắn:
- Bảng ĐẠT/VƯỢT cho từng dòng mục tiêu liên quan.
- Nếu VƯỢT: file nào trong các commit gần đây là nghi phạm chính, và một hướng điều tra.
Không sửa code sản phẩm trong skill này.
