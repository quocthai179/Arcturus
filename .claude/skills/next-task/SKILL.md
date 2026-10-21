---
name: next-task
description: Nhận và làm task tiếp theo của một làn (hub, policy, ui, lab) hoặc một task cụ thể (T0…T21) từ .squad/TASKS.md theo quy trình TDD của dự án Arcturus. Dùng khi bắt đầu một phiên làm việc hoặc sau khi xong một task.
argument-hint: "[làn hoặc mã task, ví dụ: hub | T3]"
disable-model-invocation: true
allowed-tools: Bash(git status *) Bash(git branch *) Bash(git log *)
---

Mục tiêu: hoàn thành ĐÚNG MỘT task cho làn hoặc mã task `$ARGUMENTS`.

## Bối cảnh hiện tại

Nhánh: !`git branch --show-current`

Thay đổi chưa commit:
!`git status --short`

Commit gần nhất:
!`git log --oneline -5`

## Các bước

1. Đọc `CLAUDE.md` (quy tắc bất biến, vùng sở hữu của làn), `.squad/TASKS.md`, và `docs/PLAN.md`:
   phần 1–5 (hợp đồng chung) cộng mục `### Tn` của task được chọn.
2. Chọn task:
   - Nếu `$ARGUMENTS` là mã task (T…): lấy đúng task đó.
   - Nếu là tên làn: lấy task `todo` đầu tiên của làn đó mà mọi phụ thuộc đã `done`.
   - Không có task phù hợp: báo lại các task đang chặn và dừng.
3. Đánh dấu task `doing` trong `.squad/TASKS.md`. Nếu có công cụ YouTrack: mở ticket ARC tương ứng, đọc mô tả và
   tiêu chí chấp nhận, chuyển State sang `In Progress`. Tạo nhánh `arc-<số>-<mô-tả-ngắn>`.
4. Viết kế hoạch ngắn (tối đa 8 gạch đầu dòng) dựa trên mục `### Tn` trong `docs/PLAN.md`: file sẽ sửa, test bắt buộc
   sẽ viết, rủi ro, và chỗ nào lệch khỏi PLAN.md (kèm lý do). Nếu task đụng luồng duyệt,
   policy, schema hoặc bảo mật: DỪNG và chờ người duyệt kế hoạch trước khi code.
5. Kiểm tra mọi file định sửa đều nằm trong vùng sở hữu của làn. Nếu không: ghi vào mục "Blocked" và dừng phần đó.
6. TDD: viết test cho tiêu chí chấp nhận trước (bao gồm nhánh lỗi và fail-safe), chạy cho đỏ, rồi code cho xanh.
7. Chạy kiểm tra phù hợp với phần đã đụng:
   - Rust: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
   - UI: `pnpm -C ui test`, `pnpm -C ui build`
8. Nếu đụng `crates/hub*` hoặc `crates/app`: dùng agent `budget-guard` để đo lại.
9. Tự rà theo mục "Định nghĩa xong" trong CLAUDE.md. Đụng luồng duyệt hoặc policy: gọi agent `security-auditor`
   (plugin dev-squad) rà riêng nhánh này.
10. Commit theo Conventional Commits, kèm mã ticket (`feat(hub): ARC-24 giữ request duyệt với timeout fail-safe`).
11. Cập nhật `.squad/TASKS.md`: trạng thái `review`, thêm một dòng vào Nhật ký. Nếu có công cụ YouTrack: thêm comment
    vào ticket (commit, test đã thêm, kết quả kiểm tra); chuyển State sang `In Review`; không tự chuyển sang `Done`.

Báo cáo cuối (ngắn): task đã làm, test đã thêm, kết quả kiểm tra, việc còn lại hoặc vướng mắc.
Không tự push. Không bắt đầu task thứ hai trong cùng lượt.
