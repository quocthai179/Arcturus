#!/usr/bin/env bash
# Mở các làn làm việc song song, mỗi làn một phiên Claude Code trong worktree riêng.
# Cách dùng: ./scripts/start-lanes.sh [làn ...]     (mặc định: hub policy ui lab)
# Cần: tmux, claude. Không có tmux thì script in ra lệnh để bạn tự mở từng terminal.
set -euo pipefail

lanes=("$@")
if [ ${#lanes[@]} -eq 0 ]; then
  lanes=(hub policy ui lab)
fi

repo_root="$(git rev-parse --show-toplevel)"
session="arcturus"

if ! command -v tmux >/dev/null 2>&1; then
  echo "Không thấy tmux. Mở mỗi làn trong một terminal:"
  for lane in "${lanes[@]}"; do
    echo "  cd \"$repo_root\" && claude --worktree $lane"
  done
  echo "Rồi gõ trong từng phiên: /next-task <làn>"
  exit 0
fi

if tmux has-session -t "$session" 2>/dev/null; then
  echo "Phiên tmux '$session' đã tồn tại. Gắn vào: tmux attach -t $session"
  exit 0
fi

first=1
for lane in "${lanes[@]}"; do
  if [ $first -eq 1 ]; then
    tmux new-session -d -s "$session" -n "$lane" -c "$repo_root"
    first=0
  else
    tmux new-window -t "$session" -n "$lane" -c "$repo_root"
  fi
  tmux send-keys -t "$session:$lane" "claude --worktree $lane" C-m
done

echo "Đã mở ${#lanes[@]} làn: ${lanes[*]}"
echo "Gắn vào: tmux attach -t $session   (Ctrl-b n để chuyển làn)"
echo "Trong mỗi phiên, gõ: /next-task <tên làn>"
