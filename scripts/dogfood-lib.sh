#!/usr/bin/env bash
# Phần dùng chung của dogfood-install.sh và dogfood-uninstall.sh (T18 · ARC-39). Chỉ được `source`, không chạy trực tiếp.
#
# Quy tắc (CLAUDE.md, quy tắc bất biến 7): chỉ đụng khóa `hooks` trong settings.json của Claude Code; mọi khóa khác và
# mọi hook của người dùng giữ nguyên. Luôn sao lưu `settings.json.bak-<ts>` trước khi ghi. JSON hỏng thì dừng, không ghi.
# Nhận diện hook của Arcturus theo đích của nó (`http://127.0.0.1:<cổng>/hooks/event|permission`), không theo khóa lạ
# thêm vào file (Claude Code có thể từ chối khóa không biết).

# shellcheck shell=bash
# SC2034: SETTINGS, PORT, DRY_RUN, SHOW_HELP do `parse_common_args` đặt, script gọi `source` đọc.
# SC2016: chương trình jq cố ý nằm trong nháy đơn (`$ev`, `$src` là biến của jq, không phải của shell).
# shellcheck disable=SC2034,SC2016

DOGFOOD_PORT_DEFAULT=7878

die() {
  echo "lỗi: $*" >&2
  exit 1
}

info() {
  echo "$*"
}

usage_common() {
  cat <<'EOF'
Tùy chọn:
  --settings FILE   file settings cần sửa (mặc định: ${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json)
  --port N          cổng hub (mặc định 7878, khớp `[hub] bind` trong arcturus.toml)
  --dry-run         chỉ in thay đổi, không ghi file, không sao lưu
  -h, --help        in hướng dẫn này
EOF
}

# Biểu thức chính quy (jq/Oniguruma) khớp đích hook của Arcturus.
ARC_RE='http://127\\.0\\.0\\.1:[0-9]+/hooks/(event|permission)'

# Chương trình jq: `strip` gỡ hook của Arcturus, `add($src)` gộp hook nguồn vào cuối mảng của từng sự kiện. Chỉ bỏ các
# mục/khóa trở thành rỗng vì đã gỡ hook của Arcturus; mục rỗng của người dùng giữ nguyên.
JQ_LIB='
def is_arc:
  (.type == "http" and ((.url // "") | test("^'"$ARC_RE"'$")))
  or (.type == "command" and ((.command // "") | test("'"$ARC_RE"'")));

def strip_entry:
  if (.hooks | type) == "array" then
    (.hooks) as $orig
    | ($orig | map(select(is_arc | not))) as $kept
    | if ($orig | length) > 0 and ($kept | length) == 0 then empty else .hooks = $kept end
  else . end;

def strip_arr: [ .[] | strip_entry ];

def strip_hooks:
  to_entries
  | map(. as $e
      | if ($e.value | type) == "array" then
          ($e.value | strip_arr) as $v
          | if ($e.value | length) > 0 and ($v | length) == 0 then empty else {key: $e.key, value: $v} end
        else $e end)
  | from_entries;

def strip:
  if (.hooks | type) == "object" then
    (.hooks | length) as $n
    | .hooks |= strip_hooks
    | if $n > 0 and (.hooks | length) == 0 then del(.hooks) else . end
  else . end;

def add($src):
  .hooks = ((.hooks // {}) as $cur
    | reduce ($src | keys_unsorted[]) as $ev ($cur; .[$ev] = ((.[$ev] // []) + $src[$ev])));
'

# Đọc settings (thiếu hoặc rỗng = `{}`), kiểm tra hình dạng. In JSON ra stdout.
read_settings() {
  local file="$1" text
  if [ -e "$file" ]; then
    [ -f "$file" ] || die "$file không phải file thường"
    text="$(cat "$file")" || die "không đọc được $file"
  else
    text=""
  fi
  if [ -z "$(printf '%s' "$text" | tr -d '[:space:]')" ]; then
    text='{}'
  fi
  printf '%s' "$text" | jq -e 'type == "object"' >/dev/null 2>&1 \
    || die "$file không phải JSON hợp lệ dạng object (Claude Code dùng JSON thuần, không có chú thích). Không sửa gì."
  printf '%s' "$text" | jq -e '(.hooks // {}) | type == "object" and ([.[]] | all(type == "array"))' >/dev/null 2>&1 \
    || die "khóa \"hooks\" trong $file có hình dạng lạ (cần object với mỗi sự kiện là mảng). Không sửa gì."
  printf '%s' "$text"
}

parse_common_args() {
  SETTINGS=""
  PORT="$DOGFOOD_PORT_DEFAULT"
  DRY_RUN=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --settings)
        [ $# -ge 2 ] || die "--settings cần một đường dẫn"
        SETTINGS="$2"
        shift 2
        ;;
      --port)
        [ $# -ge 2 ] || die "--port cần một số"
        case "$2" in
          '' | *[!0-9]*) die "--port phải là số nguyên (1–65535), nhận: $2" ;;
        esac
        { [ "$2" -ge 1 ] && [ "$2" -le 65535 ]; } || die "--port phải trong 1–65535, nhận: $2"
        PORT="$2"
        shift 2
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      -h | --help)
        SHOW_HELP=1
        shift
        ;;
      *)
        die "tùy chọn không biết: $1 (dùng --help)"
        ;;
    esac
  done
  if [ -z "$SETTINGS" ]; then
    local base="${CLAUDE_CONFIG_DIR:-}"
    if [ -z "$base" ]; then
      [ -n "${HOME:-}" ] || die "không có \$HOME; chỉ định --settings"
      base="$HOME/.claude"
    fi
    SETTINGS="$base/settings.json"
  fi
}

need_jq() {
  command -v jq >/dev/null 2>&1 || die "cần jq (Linux: apt install jq; macOS: brew install jq; Windows: winget install jqlang.jq)"
}

# Ghi $2 (JSON mới) vào $1 sau khi sao lưu. Ghi bằng `>` để giữ nguyên quyền, chủ sở hữu và symlink (nhiều người để
# settings.json trong repo dotfiles). Ghi lỗi giữa chừng thì khôi phục từ bản sao lưu.
commit_settings() {
  local file="$1" new="$2" backup=""
  if [ -e "$file" ]; then
    local ts n=0
    ts="$(date +%Y%m%d-%H%M%S)"
    backup="$file.bak-$ts"
    while [ -e "$backup" ]; do
      n=$((n + 1))
      backup="$file.bak-$ts-$n"
    done
    cp -p "$file" "$backup" || die "không sao lưu được $file; không sửa gì"
  else
    local dir
    dir="$(dirname "$file")"
    [ -d "$dir" ] || die "thư mục $dir không tồn tại (đã chạy claude lần nào chưa?). Tạo thư mục hoặc chỉ định --settings."
  fi
  if ! printf '%s\n' "$new" >"$file"; then
    if [ -n "$backup" ]; then
      cat "$backup" >"$file" 2>/dev/null || true
    fi
    die "ghi $file thất bại${backup:+; đã khôi phục từ $backup}"
  fi
  printf '%s' "$(cat "$file")" | jq -e . >/dev/null 2>&1 || {
    if [ -n "$backup" ]; then cat "$backup" >"$file"; fi
    die "kết quả không phải JSON hợp lệ${backup:+; đã khôi phục từ $backup}"
  }
  if [ -n "$backup" ]; then
    info "Đã sao lưu: $backup"
  fi
  info "Đã cập nhật: $file"
}

# So $2 (JSON mới) với $1 (JSON hiện tại, chuỗi) theo dạng chuẩn hóa (khóa sắp xếp): giống nhau thì không ghi gì.
# `--dry-run`: in khác biệt. Trả 0 nếu đã ghi hoặc đã in; 3 nếu không có gì đổi.
apply_change() {
  local file="$1" cur="$2" new="$3"
  if [ "$(printf '%s' "$cur" | jq -S -c .)" = "$(printf '%s' "$new" | jq -S -c .)" ]; then
    return 3
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    info "[--dry-run] Thay đổi sẽ áp dụng cho $file (không ghi gì):"
    diff -u <(printf '%s' "$cur" | jq -S .) <(printf '%s' "$new" | jq -S .) | tail -n +3 || true
    return 0
  fi
  commit_settings "$file" "$new"
}

# Đếm hook của Arcturus trong JSON $1.
count_arc() {
  printf '%s' "$1" | jq "$JQ_LIB"'[ (.hooks // {}) | .[]? | arrays | .[] | (.hooks // [])[]? | select(is_arc) ] | length'
}
