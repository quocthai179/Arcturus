# Quy trình làm việc với Claude Code

Tài liệu cho người (bạn, trưởng nhóm). Claude đọc CLAUDE.md; bạn đọc file này.

## 0. Chuẩn bị một lần

```bash
mkdir arcturus && cd arcturus && git init
# chép toàn bộ bộ quy trình này vào gốc repo
git add . && git commit -m "chore: claude code workflow"
claude            # chạy một lần để chấp nhận workspace trust
```

- Repo trên tài khoản GitHub cá nhân, máy cá nhân, ngoài giờ làm.
- Plugin `dev-squad` đã cài sẵn: dùng `code-reviewer`, `security-auditor`, `qa-engineer`, `performance-engineer` của nó.
  Bộ này chỉ thêm 2 agent riêng cho dự án (`hook-spike`, `budget-guard`), không trùng.
- Tạo token cho hub: `export ARCTURUS_TOKEN=$(openssl rand -hex 24)` và lưu vào `.env.local` (đã gitignore,
  được `.worktreeinclude` chép sang mỗi worktree).

## 1. Giai đoạn A — một phiên, đặt nền móng (tuần 1)

Chạy MỘT phiên trên nhánh chính. Chưa song song, vì các làn cần chung một nền.

1. `/next-task T0` — scaffold workspace (Cargo workspace, Tauri, Vite, CI tối thiểu).
2. `/hook-lab` — ghi lại hook thật từ 3–4 phiên Claude Code vào `fixtures/hooks/`, trả lời các câu hỏi mở trong
   `docs/HOOK_FINDINGS.md`. Đây là bước quan trọng nhất: mọi thiết kế sau dựa trên dữ liệu này.
3. `/next-task T2` — chốt `schema.rs` (hợp đồng chung). Merge vào main.

Cổng: có fixture thật, có schema đã merge, đã trả lời câu hỏi "terminal có treo khi hub giữ request không".

## 2. Giai đoạn B — chạy song song theo làn (tuần 2–5)

```bash
./scripts/start-lanes.sh     # mở 4 phiên trong tmux, mỗi phiên một worktree
```

Hoặc tự mở từng terminal:
```bash
claude --worktree hub
claude --worktree policy
claude --worktree ui
claude --worktree lab
```

Trong mỗi phiên, câu lệnh đầu tiên là `/next-task <làn>`. Skill này đọc `.squad/TASKS.md`, chọn task chưa bị chặn
của làn đó, lập kế hoạch ngắn, làm theo TDD, rồi cập nhật bảng.

### Nhịp một ngày

| Lúc | Bạn làm |
|---|---|
| Đầu phiên | Mở 2–4 làn, mỗi làn `/next-task <làn>`. Duyệt kế hoạch ngắn của từng làn (đọc lướt 1 phút). |
| Trong khi chạy | Chỉ trả lời khi có người giơ tay. Từ tuần 3, Arcturus tự hiện hàng đợi (dogfood). |
| Task xong | `/dev-squad:review` trên nhánh đó. Đụng luồng duyệt hoặc policy thì thêm agent `security-auditor`. |
| Merge | Thứ tự: `hub` → `policy` → `ui`. Các làn còn lại rebase lên main trước task tiếp theo. |
| Cuối ngày | `/budget-check` nếu có thay đổi ở hub/app. Ghi 2–3 dòng vào mục Nhật ký trong `.squad/TASKS.md`. |

### Quy tắc phối hợp giữa các làn

- **Hợp đồng trước, code sau**: đổi `schema.rs` hoặc `docs/PROTOCOL.md` là việc của làn `hub`, merge trước,
  các làn khác rebase.
- Làn cần đổi file ngoài vùng sở hữu: nhắn cho phiên sở hữu (cross-session messaging), hoặc ghi vào mục "Blocked".
- Mỗi task một nhánh, PR nhỏ (dưới khoảng 400 dòng thay đổi nếu được).
- Xong task thì `/clear` trước khi nhận task mới, để context sạch.

### Khi nào dùng chế độ plan

Bật plan mode (Shift+Tab) cho các task đụng luồng duyệt, policy, bảo mật hoặc schema. Duyệt kế hoạch rồi mới cho code.

## 3. Giai đoạn C — dogfood (từ khi T5 xong)

1. Chạy hub: `cargo run -p hub --release`.
2. Gộp `hooks/arcturus-dogfood.json` vào `~/.claude/settings.json` (phần `hooks`).
3. Từ đây, các phiên đang build Arcturus được chính Arcturus quản lý.
4. Gặp chỗ khó chịu nào khi dùng thì ghi ngay vào `.squad/TASKS.md` mục "Dogfood".

Gỡ bỏ: xóa phần `hooks` đó. Hub tắt cũng không sao: hook lỗi là lỗi không chặn, Claude Code hỏi như bình thường.

## 4. Mốc v0 (tuần 6)

Chạy `/milestone-gate v0`. Cổng đạt khi:
- Bạn dùng hằng ngày ít nhất 2 tuần liên tục.
- Thời gian trung bình xử lý một yêu cầu giảm ít nhất 30% so với chuyển tab terminal (đo trước/sau).
- 0 lần tự duyệt sai, 0 lần timeout trả allow.
- Ngân sách tài nguyên đạt trên máy yếu.

Đạt thì mở beta cho 10–20 người. Chưa đạt thì sửa trước, chưa làm VRM.

## 5. Câu lệnh mẫu

Làn `hub`:
> /next-task hub

Khi muốn chỉ định:
> Làm T3 (luồng duyệt). Bật plan trước. Viết test cho 4 nhánh: allow, deny, timeout trả {}, phiên kết thúc giữa chừng.

Làn `lab`:
> /hook-lab PermissionRequest — kiểm tra subagent có gửi PermissionRequest riêng không và trường agent_id nằm ở đâu.

Kiểm tra ngân sách:
> /budget-check
