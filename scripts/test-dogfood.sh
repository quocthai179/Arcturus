#!/usr/bin/env bash
# Test cho dogfood-install.sh / dogfood-uninstall.sh và hooks/arcturus-dogfood.json (T18 · ARC-39).
# Chỉ dùng thư mục tạm; `HOME` và `CLAUDE_CONFIG_DIR` được trỏ vào đó, nên không bao giờ đụng ~/.claude thật.
#
#   ./scripts/test-dogfood.sh

# SC2016: biểu thức jq và `bash -c` nằm trong nháy đơn có chủ ý. SC2012: tên file trong test do ta đặt (không có ký tự lạ).
# shellcheck disable=SC2016,SC2012
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
install="$here/dogfood-install.sh"
uninstall="$here/dogfood-uninstall.sh"
hooks_json="$here/../hooks/arcturus-dogfood.json"

command -v jq >/dev/null 2>&1 || { echo "cần jq" >&2; exit 2; }

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
export HOME="$root/home"
unset CLAUDE_CONFIG_DIR
mkdir -p "$HOME/.claude"

pass=0
fail=0

ok() {
  pass=$((pass + 1))
  echo "  ok   $1"
}
bad() {
  fail=$((fail + 1))
  echo "  FAIL $1" >&2
  shift
  for l in "$@"; do echo "       $l" >&2; done
}
check() { # check <tên> <lệnh test...>
  local name="$1"
  shift
  if "$@"; then ok "$name"; else bad "$name"; fi
}

fresh() { # tạo thư mục làm việc mới, in đường dẫn (mktemp, vì $(fresh) chạy trong subshell nên bộ đếm không giữ được)
  mktemp -d "$root/t.XXXXXX"
}

jq_quiet() { jq "$@" >/dev/null 2>&1; }
canon() { jq -S -c . "$1"; }
arc_count() { jq '[ (.hooks // {}) | .[]? | arrays | .[] | (.hooks // [])[]? | select((.url // .command // "") | test("127\\.0\\.0\\.1:[0-9]+/hooks/(event|permission)")) ] | length' "$1"; }
bak_count() { ls "$1".bak-* 2>/dev/null | wc -l | tr -d ' '; }

USER_HOOKS='{
  "model": "opus",
  "permissions": {"allow": ["Bash(ls)"], "deny": ["Read(./.env)"]},
  "env": {"FOO": "bar"},
  "hooks": {
    "Stop": [{"hooks": [{"type": "command", "command": "echo mine-stop"}]}],
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo mine-pre", "timeout": 5}]}],
    "Custom": []
  }
}'

echo "hooks/arcturus-dogfood.json"
check "json hợp lệ, có khóa hooks" jq_quiet -e '.hooks | type == "object"' "$hooks_json"
check "đủ 12 sự kiện" jq_quiet -e '.hooks | keys | length == 12' "$hooks_json"
check "SessionStart là hook command (hook http không chạy cho SessionStart, F1)" \
  jq_quiet -e '.hooks.SessionStart | all(.[].hooks[]; .type == "command" and (.command | contains("curl")) and (.command | contains("/hooks/event")))' "$hooks_json"
check "mọi sự kiện khác là hook http" \
  jq_quiet -e '.hooks | del(.SessionStart) | all(.[][].hooks[]; .type == "http")' "$hooks_json"
check "mọi đích là 127.0.0.1 (chỉ localhost)" \
  jq_quiet -e '[.. | strings | select(test("https?://"))] | all(test("^https?://127\\.0\\.0\\.1:7878/hooks/(event|permission)") or test("http://127\\.0\\.0\\.1:7878/hooks/event"))' "$hooks_json"
check "chỉ PermissionRequest dùng /hooks/permission, timeout 120 s" \
  jq_quiet -e '[.hooks | to_entries[] | select(.value[].hooks[] | (.url // "") | endswith("/hooks/permission")) | .key] == ["PermissionRequest"] and .hooks.PermissionRequest[0].hooks[0].timeout == 120' "$hooks_json"
check "hook sự kiện thường timeout ≤ 5 s (hub tắt không làm chậm Claude Code)" \
  jq_quiet -e '.hooks | del(.PermissionRequest) | all(.[][].hooks[]; .timeout <= 5)' "$hooks_json"
check "token chỉ qua biến môi trường, không có token cứng" \
  jq_quiet -e '[.. | strings | select(test("Bearer "))] | all(. == "Bearer $ARCTURUS_TOKEN" or contains("$ARCTURUS_TOKEN"))' "$hooks_json"

echo "cài vào settings đã có"
d="$(fresh)"
printf '%s\n' "$USER_HOOKS" >"$d/settings.json"
cp "$d/settings.json" "$d/original.json"
out="$("$install" --settings "$d/settings.json" 2>&1)"
check "cài thành công" test $? -eq 0
check "có đúng 12 hook của Arcturus" test "$(arc_count "$d/settings.json")" = 12
check "khóa khác giữ nguyên (model, permissions, env)" \
  jq_quiet -e --slurpfile o "$d/original.json" '.model == $o[0].model and .permissions == $o[0].permissions and .env == $o[0].env' "$d/settings.json"
check "hook của người dùng còn đủ, đứng trước hook của Arcturus" \
  jq_quiet -e '.hooks.Stop[0].hooks[0].command == "echo mine-stop" and .hooks.PreToolUse[0].matcher == "Bash" and .hooks.PreToolUse[0].hooks[0].command == "echo mine-pre" and (.hooks.Stop | length) == 2 and (.hooks.PreToolUse | length) == 2' "$d/settings.json"
check "mảng rỗng của người dùng giữ nguyên" jq_quiet -e '.hooks.Custom == []' "$d/settings.json"
check "có đúng một bản sao lưu, giống bản gốc" \
  bash -c '[ "$(ls "$1"/settings.json.bak-* | wc -l)" = 1 ] && cmp -s "$1"/original.json "$(ls "$1"/settings.json.bak-*)"' _ "$d"
check "thông báo nhắc ARCTURUS_TOKEN" bash -c 'grep -q ARCTURUS_TOKEN <<<"$1"' _ "$out"

echo "cài lại (idempotent)"
before="$(canon "$d/settings.json")"
out="$("$install" --settings "$d/settings.json" 2>&1)"
check "cài lần hai thành công" test $? -eq 0
check "không nhân đôi hook" test "$(arc_count "$d/settings.json")" = 12
check "nội dung không đổi" test "$(canon "$d/settings.json")" = "$before"
check "không tạo thêm bản sao lưu" test "$(bak_count "$d/settings.json")" = 1
check "báo đã cài từ trước" bash -c 'grep -q "Đã cài từ trước" <<<"$1"' _ "$out"

echo "gỡ"
out="$("$uninstall" --settings "$d/settings.json" 2>&1)"
check "gỡ thành công" test $? -eq 0
check "còn 0 hook của Arcturus" test "$(arc_count "$d/settings.json")" = 0
check "trở về đúng như ban đầu (sau chuẩn hóa)" test "$(canon "$d/settings.json")" = "$(canon "$d/original.json")"
check "bản sao lưu thứ hai tạo trước khi gỡ" test "$(bak_count "$d/settings.json")" = 2
out="$("$uninstall" --settings "$d/settings.json" 2>&1)"
check "gỡ lần hai: không có gì đổi, không thêm bản sao lưu" test "$(bak_count "$d/settings.json")" = 2
check "gỡ lần hai báo không có gì để gỡ" bash -c 'grep -q "không có gì thay đổi" <<<"$1"' _ "$out"

echo "settings chỉ có hooks của Arcturus / chưa có file / file rỗng"
d="$(fresh)"
"$install" --settings "$d/new.json" >/dev/null 2>&1
check "tạo file mới khi chưa có" test "$(arc_count "$d/new.json")" = 12
check "không sao lưu khi không có file cũ" test "$(bak_count "$d/new.json")" = 0
"$uninstall" --settings "$d/new.json" >/dev/null 2>&1
check "gỡ hết thì bỏ cả khóa hooks rỗng" test "$(canon "$d/new.json")" = '{}'
: >"$d/empty.json"
"$install" --settings "$d/empty.json" >/dev/null 2>&1
check "file rỗng coi như {}" test "$(arc_count "$d/empty.json")" = 12
"$uninstall" --settings "$d/nofile.json" >/dev/null 2>&1
check "gỡ khi không có file: thoát 0, không tạo file" bash -c '[ ! -e "$1" ]' _ "$d/nofile.json"

echo "hook của người dùng lẫn trong cùng một mục"
d="$(fresh)"
cat >"$d/settings.json" <<'EOF'
{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo mine"},{"type":"http","url":"http://127.0.0.1:7878/hooks/event","timeout":3}]}],
          "PermissionRequest":[{"hooks":[{"type":"http","url":"http://127.0.0.1:7878/hooks/permission","timeout":120}]}],
          "Notification":[{"matcher":"x","hooks":[{"type":"http","url":"http://127.0.0.1:9999/other"}]}]}}
EOF
"$uninstall" --settings "$d/settings.json" >/dev/null 2>&1
check "chỉ gỡ hook của Arcturus khỏi mục lẫn" \
  jq_quiet -e '.hooks.Stop == [{"hooks":[{"type":"command","command":"echo mine"}]}]' "$d/settings.json"
check "mục chỉ còn hook của Arcturus thì bỏ cả mục và khóa sự kiện" jq_quiet -e '.hooks | has("PermissionRequest") | not' "$d/settings.json"
check "hook http tới địa chỉ khác giữ nguyên" \
  jq_quiet -e '.hooks.Notification[0].hooks[0].url == "http://127.0.0.1:9999/other"' "$d/settings.json"

echo "cổng khác"
d="$(fresh)"
"$install" --settings "$d/settings.json" --port 7900 >/dev/null 2>&1
check "mọi đích dùng cổng 7900" jq_quiet -e '[.. | strings | select(test("127\\.0\\.0\\.1"))] | length > 0 and all(test("127\\.0\\.0\\.1:7900/"))' "$d/settings.json"
"$install" --settings "$d/settings.json" >/dev/null 2>&1
check "cài lại cổng mặc định thay hook cũ, không nhân đôi" test "$(arc_count "$d/settings.json")" = 12
check "…và dùng cổng 7878" jq_quiet -e '[.. | strings | select(test("127\\.0\\.0\\.1"))] | all(test("127\\.0\\.0\\.1:7878/"))' "$d/settings.json"
"$uninstall" --settings "$d/settings.json" >/dev/null 2>&1
"$install" --settings "$d/settings.json" --port 70000 >/dev/null 2>&1
check "cổng ngoài 1–65535 bị từ chối" test $? -ne 0
"$install" --settings "$d/settings.json" --port abc >/dev/null 2>&1
check "cổng không phải số bị từ chối" test $? -ne 0

echo "dry-run"
d="$(fresh)"
printf '%s\n' "$USER_HOOKS" >"$d/settings.json"
cp "$d/settings.json" "$d/original.json"
out="$("$install" --settings "$d/settings.json" --dry-run 2>&1)"
check "dry-run thoát 0, in khác biệt" bash -c 'grep -q "^+" <<<"$1"' _ "$out"
check "dry-run không đổi file, không sao lưu" bash -c 'cmp -s "$1/settings.json" "$1/original.json" && [ "$(ls "$1"/settings.json.bak-* 2>/dev/null | wc -l)" = 0 ]' _ "$d"

echo "đầu vào hỏng"
d="$(fresh)"
for name in 'không phải JSON' '[1,2]' '{"hooks": []}' '{"hooks": {"Stop": {"a": 1}}}' '{"a": 1,}'; do
  printf '%s' "$name" >"$d/bad.json"
  cp "$d/bad.json" "$d/bad.orig"
  "$install" --settings "$d/bad.json" >/dev/null 2>&1
  rc=$?
  check "từ chối và không ghi: $name" bash -c '[ "$1" -ne 0 ] && cmp -s "$2/bad.json" "$2/bad.orig" && [ "$(ls "$2"/bad.json.bak-* 2>/dev/null | wc -l)" = 0 ]' _ "$rc" "$d"
done
"$install" --settings "$root/khong-co-thu-muc/settings.json" >/dev/null 2>&1
check "thư mục đích không tồn tại: báo lỗi" test $? -ne 0
"$install" --bogus >/dev/null 2>&1
check "tùy chọn lạ: báo lỗi" test $? -ne 0

echo "symlink và quyền file"
d="$(fresh)"
mkdir -p "$d/dotfiles"
printf '%s\n' "$USER_HOOKS" >"$d/dotfiles/settings.json"
chmod 600 "$d/dotfiles/settings.json"
ln -s "$d/dotfiles/settings.json" "$d/settings.json"
"$install" --settings "$d/settings.json" >/dev/null 2>&1
check "symlink còn nguyên, ghi vào file đích" bash -c '[ -L "$1/settings.json" ] && jq -e ".hooks.SessionStart" "$1/dotfiles/settings.json" >/dev/null' _ "$d"
check "quyền 600 giữ nguyên" bash -c '[ "$(stat -c %a "$1/dotfiles/settings.json" 2>/dev/null || stat -f %Lp "$1/dotfiles/settings.json")" = 600 ]' _ "$d"

echo "đường dẫn mặc định"
printf '%s\n' '{"model":"x"}' >"$HOME/.claude/settings.json"
"$install" >/dev/null 2>&1
check "mặc định ghi \$HOME/.claude/settings.json (HOME đã trỏ vào thư mục tạm)" test "$(arc_count "$HOME/.claude/settings.json")" = 12
mkdir -p "$root/cfg"
CLAUDE_CONFIG_DIR="$root/cfg" "$install" >/dev/null 2>&1
check "CLAUDE_CONFIG_DIR được tôn trọng" test "$(arc_count "$root/cfg/settings.json")" = 12
CLAUDE_CONFIG_DIR="$root/cfg" "$uninstall" >/dev/null 2>&1
"$uninstall" >/dev/null 2>&1
check "gỡ khỏi cả hai nơi" bash -c '[ "$(jq "[.. | strings | select(test(\"/hooks/\"))] | length" "$1")" = 0 ] && [ "$(jq "[.. | strings | select(test(\"/hooks/\"))] | length" "$2")" = 0 ]' _ "$HOME/.claude/settings.json" "$root/cfg/settings.json"
check "khóa khác trong settings mặc định còn nguyên" jq_quiet -e '.model == "x"' "$HOME/.claude/settings.json"

echo
echo "đạt: $pass, hỏng: $fail"
[ "$fail" -eq 0 ]
