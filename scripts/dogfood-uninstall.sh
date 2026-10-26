#!/usr/bin/env bash
# Tắt dogfood: gỡ hook của Arcturus khỏi settings.json của Claude Code (T18 · ARC-39).
#
#   ./scripts/dogfood-uninstall.sh [--settings FILE] [--dry-run]
#
# - Chỉ gỡ hook trỏ tới hub của Arcturus (http://127.0.0.1:<cổng>/hooks/event|permission); hook của bạn giữ nguyên.
# - Mục/khóa trở thành rỗng vì việc gỡ thì bị bỏ; khóa `hooks` rỗng thì bị bỏ.
# - Sao lưu `settings.json.bak-<ts>` trước khi ghi. Không có gì để gỡ thì không ghi, không sao lưu.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/dogfood-lib.sh
. "$here/dogfood-lib.sh"

SHOW_HELP=0
parse_common_args "$@"
if [ "$SHOW_HELP" -eq 1 ]; then
  echo "Dùng: $(basename "$0") [tùy chọn]"
  echo "Gỡ hook của Arcturus khỏi settings.json của Claude Code (có sao lưu)."
  echo
  usage_common
  exit 0
fi

need_jq
if [ ! -e "$SETTINGS" ]; then
  info "Không có $SETTINGS, không có gì để gỡ."
  exit 0
fi

cur="$(read_settings "$SETTINGS")"
before="$(count_arc "$cur")"
new="$(printf '%s' "$cur" | jq "$JQ_LIB"'strip')" || die "không dựng được settings mới"

status=0
apply_change "$SETTINGS" "$cur" "$new" || status=$?
if [ "$status" -eq 3 ]; then
  info "Không thấy hook của Arcturus trong $SETTINGS, không có gì thay đổi."
elif [ "$status" -ne 0 ]; then
  exit "$status"
elif [ "$DRY_RUN" -eq 0 ]; then
  info "Đã gỡ $before hook của Arcturus. Mở phiên Claude Code mới để áp dụng; phiên đang chạy giữ hook cũ tới khi đóng."
fi
