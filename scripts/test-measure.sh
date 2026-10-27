#!/usr/bin/env bash
# Test phần phân tích của scripts/measure.sh bằng CSV giả (không khởi động hub). Chạy: bash scripts/test-measure.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

csv() { # $1 = tên, các đối số sau: "rss_kb cpu pha"
  local f="$WORK/$1"
  shift
  echo 'ts,rss_kb,cpu_pct,priv_kb,phase' >"$f"
  local i=0 row rss cpu phase
  for row in "$@"; do
    read -r rss cpu phase <<<"$row"
    printf '2026-10-27T10:00:%02d.000Z,%s,%s,%s,%s\n' "$i" "$rss" "$cpu" "$((rss / 4))" "$phase" >>"$f"
    i=$((i + 1))
  done
  echo "$f"
}

check() { # $1 = tên, $2 = mã thoát mong đợi, $3 = mẫu cần có trong đầu ra, $4.. = lệnh
  local name="$1" want_code="$2" pattern="$3" out code
  shift 3
  out="$("$@" 2>&1)"
  code=$?
  if [ "$code" -eq "$want_code" ] && printf '%s' "$out" | grep -Eq -- "$pattern"; then
    echo "ok   $name"
  else
    echo "FAIL $name (mã $code): $out"
    fail=$((fail + 1))
  fi
}

M="$HERE/measure.sh"
good="$(csv good.csv '12288 0.00 idle' '12288 0.00 idle' '30720 1.50 load' '30720 1.50 load' '13312 0.00 rest')"
MEASURE_INGEST_P99_MS=0.93 check 'hub đạt: PASS' 0 'verdict=PASS' bash "$M" --analyze "$good"
MEASURE_INGEST_P99_MS=0.93 check 'hub: số liệu trong dòng tóm tắt' 0 'idle_rss_mb=12\.0.*load_peak_rss_mb=30\.0.*rest_rss_mb=13\.0.*ingest_p99_ms=0\.93' bash "$M" --analyze "$good"
check 'hub FAIL: idle vượt 15 MB' 1 'idle_rss>15MB' bash "$M" --analyze "$(csv a.csv '18432 0.00 idle' '18432 0.00 rest')"
check 'hub FAIL: đỉnh vượt 40 MB' 1 'load_peak>40MB' bash "$M" --analyze "$(csv b.csv '12288 0.00 idle' '46080 2.00 load' '12288 0.00 rest')"
check 'hub FAIL: CPU idle vượt 0.5%' 1 'idle_cpu>0\.5%' bash "$M" --analyze "$(csv c.csv '12288 3.00 idle' '12288 0.00 rest')"
check 'hub FAIL: rò rỉ sau khi nghỉ' 1 'ro_ri>5MB' bash "$M" --analyze "$(csv d.csv '12288 0.00 idle' '30720 1.00 load' '25600 0.00 rest')"
MEASURE_INGEST_P99_MS=9.5 check 'hub FAIL: p99 vượt 5 ms' 1 'p99>5ms' bash "$M" --analyze "$(csv e.csv '12288 0.00 idle')"
check 'hub FAIL: thiếu mẫu idle' 1 'khong_co_mau_idle' bash "$M" --analyze "$(csv f.csv '12288 0.00 load')"
check 'target app: báo chưa hỗ trợ' 2 'measure.ps1' bash "$M" --target app

if [ "$fail" -gt 0 ]; then
  echo "$fail kiểm tra lỗi"
  exit 1
fi
echo 'tất cả kiểm tra đạt'
