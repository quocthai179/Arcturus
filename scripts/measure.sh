#!/usr/bin/env bash
# Đo RSS/CPU/độ trễ của hub Arcturus theo kịch bản chuẩn của docs/BUDGET.md (docs/PLAN.md T16 · ARC-37). Bản Linux/macOS;
# bản Windows (hub và app) là scripts/measure.ps1, cùng định dạng CSV và dòng tóm tắt.
#
#   scripts/measure.sh                         hub: idle 60 s, tải 120 s (20 phiên, 50 yêu cầu/s), nghỉ 60 s
#   scripts/measure.sh --analyze out/hub.csv   chỉ phân tích một CSV (env MEASURE_INGEST_P99_MS cho p99)
#
# Tuỳ chọn: --idle-sec --load-sec --rest-sec --sessions --rate --interval-sec --port --hub-bin --replay-bin --fixtures
#           --out-dir --no-build
# CSV: ts,rss_kb,cpu_pct,priv_kb,phase (cpu_pct tính theo một nhân). In đúng một dòng tóm tắt "measure ..." kết thúc bằng
# verdict=PASS|FAIL; mã thoát 1 khi FAIL. App (Tauri) chưa đo được trên Linux/macOS ở v0: dùng measure.ps1 trên Windows.
set -euo pipefail

# Ngân sách (docs/BUDGET.md). Đổi ở đây phải đổi cả bảng "Mục tiêu" và measure.ps1.
HUB_IDLE_MB=15
HUB_LOAD_MB=40
IDLE_CPU_PCT=0.5
P99_INGEST_MS=5
LEAK_TOLERANCE_MB=5

IDLE_SEC=60
LOAD_SEC=120
REST_SEC=60
SESSIONS=20
RATE=50
INTERVAL_SEC=5
PORT=7899
HUB_BIN=""
REPLAY_BIN=""
FIXTURES="fixtures/hooks"
OUT_DIR=""
NO_BUILD=0
ANALYZE=""
TARGET="hub"

usage() { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --analyze) ANALYZE="$2"; shift 2 ;;
    --idle-sec) IDLE_SEC="$2"; shift 2 ;;
    --load-sec) LOAD_SEC="$2"; shift 2 ;;
    --rest-sec) REST_SEC="$2"; shift 2 ;;
    --sessions) SESSIONS="$2"; shift 2 ;;
    --rate) RATE="$2"; shift 2 ;;
    --interval-sec) INTERVAL_SEC="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --hub-bin) HUB_BIN="$2"; shift 2 ;;
    --replay-bin) REPLAY_BIN="$2"; shift 2 ;;
    --fixtures) FIXTURES="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) echo "measure.sh: tuỳ chọn lạ: $1" >&2; usage >&2; exit 2 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Phân tích CSV → dòng tóm tắt (thuần awk, test được). $1 = CSV, $2 = p99 ingest (ms), $3 = commit.
summarize() {
  awk -F, -v csv_p99="$2" -v commit="$3" \
    -v idle_max="$HUB_IDLE_MB" -v load_max="$HUB_LOAD_MB" -v cpu_max="$IDLE_CPU_PCT" -v p99_max="$P99_INGEST_MS" -v leak="$LEAK_TOLERANCE_MB" '
    NR == 1 { next }
    {
      mb = $2 / 1024.0
      ph = $5
      gsub(/\r/, "", ph)
      n[ph]++; sum[ph] += mb; cpu[ph] += $3
      if (mb > peak[ph]) peak[ph] = mb
    }
    END {
      idle_rss = n["idle"] ? sum["idle"] / n["idle"] : 0
      rest_rss = n["rest"] ? sum["rest"] / n["rest"] : 0
      load_peak = peak["load"] + 0
      load_cpu = n["load"] ? cpu["load"] / n["load"] : 0
      cn = n["idle"] + n["rest"]
      idle_cpu = cn ? (cpu["idle"] + cpu["rest"]) / cn : 0
      reasons = ""
      if (n["idle"] == 0) reasons = reasons ",khong_co_mau_idle"
      if (idle_rss > idle_max) reasons = reasons ",idle_rss>" idle_max "MB"
      if (n["load"] > 0 && load_peak > load_max) reasons = reasons ",load_peak>" load_max "MB"
      if (idle_cpu > cpu_max) reasons = reasons ",idle_cpu>" cpu_max "%"
      if (n["rest"] > 0 && rest_rss > idle_rss + leak) reasons = reasons ",ro_ri>" leak "MB"
      if (csv_p99 + 0 > p99_max) reasons = reasons ",p99>" p99_max "ms"
      verdict = reasons == "" ? "PASS" : "FAIL"
      if (reasons == "") reasons = "-"; else reasons = substr(reasons, 2)
      printf "measure target=hub commit=%s idle_rss_mb=%.1f idle_cpu_pct=%.2f load_peak_rss_mb=%.1f load_cpu_avg_pct=%.2f rest_rss_mb=%.1f ingest_p99_ms=%.2f reasons=%s verdict=%s\n", \
        commit, idle_rss, idle_cpu, load_peak, load_cpu, rest_rss, csv_p99, reasons, verdict
    }' "$1"
}

if [ -n "$ANALYZE" ]; then
  line="$(summarize "$ANALYZE" "${MEASURE_INGEST_P99_MS:-0}" analyze)"
  echo "$line"
  case "$line" in *verdict=FAIL) exit 1 ;; *) exit 0 ;; esac
fi

if [ "$TARGET" != "hub" ]; then
  echo "measure.sh: chỉ đo hub; app (Tauri) đo bằng scripts/measure.ps1 trên Windows" >&2
  exit 2
fi

# --- Lấy mẫu -----------------------------------------------------------------------------------------------------

rss_kb() { # $1 = pid
  if [ -r "/proc/$1/status" ]; then
    awk '/^VmRSS:/ { print $2 }' "/proc/$1/status"
  else
    ps -o rss= -p "$1" | tr -d ' '
  fi
}

priv_kb() { # RssAnon trên Linux; nơi khác dùng RSS
  if [ -r "/proc/$1/status" ]; then
    awk '/^RssAnon:/ { print $2 }' "/proc/$1/status"
  else
    rss_kb "$1"
  fi
}

cpu_centis() { # CPU đã dùng, đơn vị 1/100 giây
  if [ -r "/proc/$1/stat" ]; then
    local tck
    tck="$(getconf CLK_TCK)"
    # Trường 14/15 sau tên tiến trình (tên có thể chứa khoảng trắng nên cắt sau dấu ')' cuối).
    sed 's/^.*) //' "/proc/$1/stat" | awk -v tck="$tck" '{ printf "%d\n", ($12 + $13) * 100 / tck }'
  else
    # macOS: TIME dạng [[dd-]hh:]mm:ss[.cc]
    ps -o time= -p "$1" | awk '{
      t = $1; d = 0
      if (index(t, "-")) { split(t, a, "-"); d = a[1]; t = a[2] }
      m = split(t, p, ":")
      s = p[m] + 0; mi = (m >= 2) ? p[m - 1] : 0; h = (m >= 3) ? p[m - 2] : 0
      printf "%d\n", (((d * 24 + h) * 60 + mi) * 60 + s) * 100
    }'
  fi
}

now_ms() { date +%s%3N 2>/dev/null | grep -E '^[0-9]+$' || echo $(($(date +%s) * 1000)); }

ROWS_FILE=""
PREV_MS=0
PREV_CPU=0

sample() { # $1 = pid, $2 = pha
  local pid="$1" phase="$2" now cpu pct rss priv
  now="$(now_ms)"
  cpu="$(cpu_centis "$pid")"
  rss="$(rss_kb "$pid")"
  priv="$(priv_kb "$pid")"
  pct="0.00"
  if [ "$PREV_MS" -gt 0 ] && [ "$now" -gt "$PREV_MS" ]; then
    pct="$(awk -v dc="$((cpu - PREV_CPU))" -v dt="$((now - PREV_MS))" 'BEGIN { v = dc * 10.0 / dt * 100.0; if (v < 0) v = 0; printf "%.2f", v }')"
  fi
  PREV_MS="$now"
  PREV_CPU="$cpu"
  if [ -n "$phase" ] && [ -n "$ROWS_FILE" ]; then
    printf '%s,%s,%s,%s,%s\n' "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" "$rss" "$pct" "$priv" "$phase" >>"$ROWS_FILE"
    printf '[%s] %-5s rss=%s KB cpu=%s%%\n' "$(date +%H:%M:%S)" "$phase" "$rss" "$pct"
  fi
}

sample_for() { # $1 = pid, $2 = pha, $3 = số giây
  local end=$((SECONDS + $3))
  while [ "$SECONDS" -lt "$end" ]; do
    sleep "$INTERVAL_SEC"
    sample "$1" "$2"
  done
}

# --- Kịch bản hub ------------------------------------------------------------------------------------------------

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/target/measure"
mkdir -p "$OUT_DIR"
WORK="$(mktemp -d)"
HUB_PID=""
REPLAY_PID=""
# shellcheck disable=SC2329 # gọi qua trap
cleanup() {
  if [ -n "$REPLAY_PID" ]; then kill "$REPLAY_PID" 2>/dev/null || true; fi
  if [ -n "$HUB_PID" ]; then kill "$HUB_PID" 2>/dev/null || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -n "$HUB_BIN" ] || HUB_BIN="$ROOT/target/release/hub"
[ -n "$REPLAY_BIN" ] || REPLAY_BIN="$ROOT/target/release/replay"
if [ ! -x "$HUB_BIN" ] || [ ! -x "$REPLAY_BIN" ]; then
  if [ "$NO_BUILD" -eq 1 ]; then
    echo "measure.sh: thiếu $HUB_BIN hoặc $REPLAY_BIN (đã bật --no-build)" >&2
    exit 2
  fi
  cargo build --release -p hub -p replay --manifest-path "$ROOT/Cargo.toml"
fi
case "$FIXTURES" in /*) FIX="$FIXTURES" ;; *) FIX="$ROOT/$FIXTURES" ;; esac

COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
if [ -z "${ARCTURUS_TOKEN:-}" ]; then
  ARCTURUS_TOKEN="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
fi
export ARCTURUS_TOKEN

"$HUB_BIN" --bind "127.0.0.1:$PORT" --db "$WORK/arcturus.db" --ui-token-file "$WORK/ui-token" >"$WORK/hub.out" 2>"$WORK/hub.err" &
HUB_PID=$!
sleep 2
if ! kill -0 "$HUB_PID" 2>/dev/null; then
  echo "measure.sh: hub thoát sớm: $(cat "$WORK/hub.err")" >&2
  exit 2
fi
echo "Hub pid $HUB_PID trên cổng $PORT; idle ${IDLE_SEC}s, tải ${LOAD_SEC}s ($SESSIONS phiên, $RATE/s), nghỉ ${REST_SEC}s"

STAMP="$(date +%Y%m%d-%H%M%S)"
CSV="$OUT_DIR/hub-$STAMP.csv"
ROWS_FILE="$CSV"
echo 'ts,rss_kb,cpu_pct,priv_kb,phase' >"$CSV"
sample "$HUB_PID" ""   # mẫu gốc để tính CPU; không ghi vào CSV

sample_for "$HUB_PID" idle "$IDLE_SEC"

"$REPLAY_BIN" "$FIX" --url "http://127.0.0.1:$PORT" --sessions "$SESSIONS" --rate "$RATE" --duration "$LOAD_SEC" --permission-mode skip \
  >"$WORK/replay.out" 2>"$WORK/replay.err" &
REPLAY_PID=$!
load_end=$((SECONDS + LOAD_SEC + 15))
while kill -0 "$REPLAY_PID" 2>/dev/null && [ "$SECONDS" -lt "$load_end" ]; do
  sleep "$INTERVAL_SEC"
  sample "$HUB_PID" load
done
kill "$REPLAY_PID" 2>/dev/null || true
wait "$REPLAY_PID" 2>/dev/null || true
REPLAY_PID=""
cat "$WORK/replay.out"
P99="$(sed -n 's|.*/hooks/event: [0-9]* yêu cầu, [0-9]* lỗi, p50 [0-9.,]* ms, p95 [0-9.,]* ms, p99 \([0-9.,]*\) ms.*|\1|p' "$WORK/replay.out" | head -n1 | tr ',' '.')"

sample_for "$HUB_PID" rest "$REST_SEC"

echo "CSV: $CSV"
line="$(summarize "$CSV" "${P99:-0}" "$COMMIT")"
echo "$line"
case "$line" in *verdict=FAIL) exit 1 ;; *) exit 0 ;; esac
