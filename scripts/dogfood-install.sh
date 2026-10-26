#!/usr/bin/env bash
# Bật dogfood: gộp hook của Arcturus (hooks/arcturus-dogfood.json) vào settings.json của Claude Code (T18 · ARC-39).
#
#   ./scripts/dogfood-install.sh [--settings FILE] [--port N] [--dry-run]
#
# - Chỉ đụng khóa `hooks`; hook của bạn giữ nguyên, hook của Arcturus được thêm vào cuối mảng của từng sự kiện.
# - Chạy lại nhiều lần an toàn (gỡ bản cũ của Arcturus rồi thêm bản mới, không nhân đôi).
# - Sao lưu `settings.json.bak-<ts>` trước khi ghi. JSON hỏng thì dừng, không ghi.
# - Gỡ bằng ./scripts/dogfood-uninstall.sh. Hướng dẫn đầy đủ: docs/DOGFOOD.md.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/dogfood-lib.sh
. "$here/dogfood-lib.sh"

SHOW_HELP=0
HOOKS_SRC="$here/../hooks/arcturus-dogfood.json"
parse_common_args "$@"
if [ "$SHOW_HELP" -eq 1 ]; then
  echo "Dùng: $(basename "$0") [tùy chọn]"
  echo "Gộp hook của Arcturus vào settings.json của Claude Code (có sao lưu)."
  echo
  usage_common
  exit 0
fi

need_jq
[ -f "$HOOKS_SRC" ] || die "không thấy $HOOKS_SRC"
src="$(jq -c '.hooks' "$HOOKS_SRC")" || die "$HOOKS_SRC không phải JSON hợp lệ"
printf '%s' "$src" | jq -e 'type == "object" and length > 0' >/dev/null || die "$HOOKS_SRC thiếu khóa \"hooks\""

# Đổi cổng nếu không dùng 7878 (chỉ chuỗi chứa địa chỉ hub).
if [ "$PORT" != "$DOGFOOD_PORT_DEFAULT" ]; then
  src="$(printf '%s' "$src" | jq -c --arg p "$PORT" \
    'walk(if type == "string" then gsub("127\\.0\\.0\\.1:7878"; "127.0.0.1:" + $p) else . end)')"
fi

cur="$(read_settings "$SETTINGS")"
new="$(printf '%s' "$cur" | jq "$JQ_LIB"'strip | add($src)' --argjson src "$src")" || die "không dựng được settings mới"

status=0
apply_change "$SETTINGS" "$cur" "$new" || status=$?
if [ "$status" -eq 3 ]; then
  info "Đã cài từ trước, không có gì thay đổi ($SETTINGS)."
elif [ "$status" -ne 0 ]; then
  exit "$status"
fi

n="$(count_arc "$new")"
if [ "$DRY_RUN" -eq 0 ]; then
  info "Hook của Arcturus trong settings: $n (cổng $PORT)."
  echo
  echo "Việc còn lại:"
  if [ -z "${ARCTURUS_TOKEN:-}" ]; then
    echo "  1. Đặt ARCTURUS_TOKEN (cùng giá trị cho hub và cho claude), ví dụ: export ARCTURUS_TOKEN=\$(openssl rand -hex 24)"
    echo "     Không đặt thì hub không khởi động và hook không xác thực được (hook lỗi là lỗi không chặn)."
  else
    echo "  1. ARCTURUS_TOKEN đã đặt trong shell này; phải đặt cùng giá trị ở nơi khởi động hub/app và claude."
  fi
  echo "  2. Chạy app (tray) hoặc hub, rồi mở một phiên Claude Code MỚI (hook đọc lúc phiên bắt đầu)."
  if command -v curl >/dev/null 2>&1; then
    if [ "$(curl -s -m 1 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/healthz" 2>/dev/null || true)" = "200" ]; then
      echo "  (hub đang chạy ở cổng $PORT)"
    else
      echo "  (chưa thấy hub ở cổng $PORT; hook sẽ lỗi không chặn tới khi hub chạy)"
    fi
  fi
  echo "  Chi tiết và cách kiểm tra: docs/DOGFOOD.md"
fi
