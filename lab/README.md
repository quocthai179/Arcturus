# lab/ — thí nghiệm hook (T1)

`settings.local.json` trỏ mọi hook về hub ghi băng ở `127.0.0.1:7879` (cổng riêng, không đụng hub thật ở 7878).
Chỉ nạp qua cờ `--settings`, không gộp vào `~/.claude/settings.json`.

```bash
cargo build -p hub -p replay
ARCTURUS_TOKEN=<token> target/debug/hub --record <thư-mục-tạm>/rec --bind 127.0.0.1:7879 [--record-delay 20]
ARCTURUS_TOKEN=<token> claude -p "..." --settings lab/settings.local.json --allowedTools "Read" "Bash(echo *)"
ARCTURUS_TOKEN=<token> target/debug/replay scrub <thư-mục-tạm>/rec fixtures/hooks --home "$HOME"
```

- Phiên headless nên chạy trong một thư mục tạm ngoài repo, để settings và CLAUDE.md của dự án không ảnh hưởng.
- Muốn có `PermissionRequest`, chạy với `--permission-mode default`: chế độ auto không hỏi nên không gửi hook này.
- Hook http cho `SessionStart` không bắn; muốn ghi SessionStart, dùng hook `type: "command"` gọi `curl`
  (`docs/HOOK_FINDINGS.md` F1).
- Chế độ ghi băng không giới hạn số file hay số kết nối đang giữ (`--record-delay`): chỉ chạy khi thí nghiệm, tắt ngay
  sau đó. Ghi vào thư mục tạm ngoài repo; `replay scrub` từ chối IN/OUT lồng nhau.
- Rà lại fixture sau khi scrub (home, slug `C--Users-<tên>`, token, tên máy) trước khi commit.
