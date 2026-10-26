# Arcturus — buồng lái trưởng nhóm cho đội AI agent

Hub nhẹ chạy trên máy bạn, gom sự kiện từ nhiều phiên Claude Code để bạn thấy ai đang làm gì, ai đang giơ tay xin
duyệt, rồi duyệt/từ chối từ một chỗ. Mỗi agent là một nhân vật 2D. Chi tiết: `CLAUDE.md`, `docs/ARCHITECTURE.md`.

**Trạng thái:** v0 đang ở giai đoạn kế hoạch, chưa có code. Việc đầu tiên là task T0 (scaffold) trong `.squad/TASKS.md`.

## Tài liệu và cấu hình

| File | Dành cho | Vai trò |
|---|---|---|
| `CLAUDE.md` | Claude | Bối cảnh dự án, 7 quy tắc bất biến, vùng sở hữu từng làn, định nghĩa "xong" |
| `docs/WORKFLOW.md` | Bạn | Quy trình 3 giai đoạn, nhịp một ngày, câu lệnh mẫu |
| `docs/PLAN.md` | Claude | Kế hoạch triển khai: cấu trúc module, kiểu dữ liệu, endpoint, schema DB, thuật toán policy, test bắt buộc cho từng task T0–T21, SVG nhân vật |
| `docs/ARCHITECTURE.md` | Cả hai | Thành phần, luồng duyệt, schema sự kiện, bảng SQLite (tóm tắt; đặc tả ở `docs/PLAN.md` §3) |
| `docs/BUDGET.md` | Cả hai | Ngân sách RAM/CPU, kịch bản đo, lịch sử đo |
| `docs/HOOK_FINDINGS.md` | Cả hai | 10 câu hỏi về hook cần kiểm chứng trước khi build |
| `.squad/TASKS.md` | Cả hai | 22 task v0 chia theo làn, có phụ thuộc và tiêu chí chấp nhận |
| `.claude/settings.json` | Claude Code | Quyền: cho phép build/test, hỏi khi push, chặn lệnh nguy hiểm và đọc bí mật |
| `.claude/agents/hook-spike.md` | Claude | Agent thực nghiệm hành vi hook, ghi bằng chứng |
| `.claude/agents/budget-guard.md` | Claude | Agent đo ngân sách tài nguyên |
| `.claude/skills/next-task/` | Bạn gõ `/next-task hub` | Nhận một task, TDD, kiểm tra, commit, cập nhật bảng |
| `.claude/skills/hook-lab/` | `/hook-lab Q1` | Chạy thực nghiệm hook |
| `.claude/skills/budget-check/` | `/budget-check` | Đo và so ngân sách |
| `.claude/skills/milestone-gate/` | `/milestone-gate v0` | Cổng chất lượng trước khi phát hành |
| `hooks/arcturus-dogfood.json` | Bạn | Hook để Claude Code đẩy sự kiện vào hub |
| `scripts/dogfood-install.sh`, `dogfood-uninstall.sh` | Bạn | Gộp/gỡ hook trong `~/.claude/settings.json` (có sao lưu); hướng dẫn: `docs/DOGFOOD.md` |
| `scripts/start-lanes.sh` | Bạn | Mở 4 phiên song song trong tmux, mỗi phiên một worktree |
| `arcturus.example.toml` | Bạn | Gán tên, vai trò, màu nhân vật theo worktree |

Agent dùng chung như `code-reviewer`, `security-auditor`, `qa-engineer` lấy từ plugin dev-squad đã cài, không định nghĩa lại.

## Bắt đầu nhanh

```bash
git clone <url-repo-arcturus> arcturus && cd arcturus
mkdir -p ~/.config/arcturus && cp arcturus.example.toml ~/.config/arcturus/arcturus.toml   # Windows: %APPDATA%\arcturus\
chmod +x scripts/start-lanes.sh
claude
> /next-task T0
```

Quy trình đầy đủ: `docs/WORKFLOW.md`.
