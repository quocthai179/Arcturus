-- Chỉ số dogfood của Arcturus (docs/PLAN.md T20 · ARC-41). Chạy trên cơ sở dữ liệu của hub (khi hub đang tắt hoặc đang chạy, WAL
-- cho phép đọc song song):
--
--   sqlite3 -header -column "$HOME/.local/share/arcturus/arcturus.db" < scripts/metrics.sql
--   (Windows: %APPDATA%\arcturus\arcturus.db; hub in đường dẫn ở dòng log đầu tiên: db=...)
--
-- Mỗi truy vấn bắt đầu bằng dòng "-- @query <tên>" (test `metrics_sql` chạy từng truy vấn trên schema thật).
-- Chỉ ĐỌC. Thời gian lưu là UTC, mili giây ("2026-10-27T06:36:46.123Z"), nên cắt 10 ký tự đầu là ngày UTC.
-- Muốn giới hạn khoảng ngày: thêm `WHERE created_at >= '2026-10-27'` (approvals) hoặc `ts >= ...` (events).
--
-- Cách đọc:
--   * wait_s = latency_ms/1000: từ lúc yêu cầu vào hub tới lúc có quyết định. Chỉ tính yêu cầu có quyết định (allowed, denied);
--     hết giờ, hủy và "trả về terminal" không có người quyết nên không đưa vào trung bình.
--   * user = người bấm hoặc hết giờ đếm ngược phủ quyết (decided_by user, countdown); policy = tự duyệt.
--   * "Yêu cầu" chỉ gồm thứ Claude Code thật sự hỏi người (lệnh đã cho phép sẵn không tới hub, docs/DOGFOOD.md mục 8).

-- @query per_day
-- Mỗi ngày: số yêu cầu, thời gian chờ trung bình, tỉ lệ policy, số deny, số timeout.
SELECT substr(created_at, 1, 10)                                                         AS day,
       COUNT(*)                                                                          AS requests,
       SUM(status IN ('allowed', 'denied'))                                              AS decided,
       ROUND(AVG(CASE WHEN status IN ('allowed', 'denied') THEN latency_ms END) / 1000.0, 2)
                                                                                         AS avg_wait_all_s,
       ROUND(AVG(CASE WHEN status IN ('allowed', 'denied') AND decided_by IN ('user', 'countdown')
                      THEN latency_ms END) / 1000.0, 2)                                  AS avg_wait_user_s,
       ROUND(MAX(CASE WHEN status IN ('allowed', 'denied') AND decided_by IN ('user', 'countdown')
                      THEN latency_ms END) / 1000.0, 2)                                  AS max_wait_user_s,
       ROUND(100.0 * SUM(decided_by = 'policy') / NULLIF(SUM(status IN ('allowed', 'denied')), 0), 1)
                                                                                         AS policy_pct,
       SUM(status = 'denied')                                                            AS denied,
       SUM(status = 'timed_out')                                                         AS timed_out,
       SUM(status = 'handed_off')                                                        AS handed_off,
       SUM(status = 'cancelled')                                                         AS cancelled
FROM approvals
GROUP BY day
ORDER BY day;

-- @query total
-- Toàn bộ khoảng dữ liệu (một dòng): dùng để so với baseline trong docs/DOGFOOD.md.
SELECT COUNT(*)                                                                          AS requests,
       SUM(status IN ('allowed', 'denied'))                                              AS decided,
       ROUND(AVG(CASE WHEN status IN ('allowed', 'denied') THEN latency_ms END) / 1000.0, 2)
                                                                                         AS avg_wait_all_s,
       ROUND(AVG(CASE WHEN status IN ('allowed', 'denied') AND decided_by IN ('user', 'countdown')
                      THEN latency_ms END) / 1000.0, 2)                                  AS avg_wait_user_s,
       ROUND(100.0 * SUM(decided_by = 'policy') / NULLIF(SUM(status IN ('allowed', 'denied')), 0), 1)
                                                                                         AS policy_pct,
       SUM(status = 'denied')                                                            AS denied,
       SUM(status = 'timed_out')                                                         AS timed_out,
       COUNT(DISTINCT substr(created_at, 1, 10))                                         AS days_with_data
FROM approvals;

-- @query auto_approved_risk
-- Tự duyệt (policy) theo mức rủi ro. Tiêu chí cổng v0: 0 lần tự duyệt sai; mức high/critical KHÔNG được có dòng nào ở đây.
SELECT risk,
       COUNT(*)           AS auto_allowed,
       MIN(created_at)    AS first_at,
       MAX(created_at)    AS last_at
FROM approvals
WHERE decided_by = 'policy' AND status = 'allowed'
GROUP BY risk
ORDER BY CASE risk WHEN 'critical' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 ELSE 3 END;

-- @query auto_approved_dangerous
-- Danh sách cần xem tay: mọi yêu cầu rủi ro high/critical mà policy đã tự allow (phải rỗng).
SELECT request_id, created_at, risk, tool, substr(command_normalized, 1, 120) AS command
FROM approvals
WHERE decided_by = 'policy' AND status = 'allowed' AND risk IN ('high', 'critical')
ORDER BY created_at;

-- @query agent_self_run
-- Thời gian agent tự chạy mỗi lượt: một lượt là cặp liền nhau (prompt.submitted → turn.stopped) của cùng phiên.
--   avg_turn_s     thời gian cả lượt
--   avg_wait_s     tổng thời gian các yêu cầu có quyết định rơi trong lượt (yêu cầu song song cộng dồn)
--   avg_self_run_s thời gian lượt trừ thời gian chờ duyệt (không âm): agent chạy mà không bị chặn
WITH ev AS (
  SELECT session_id, kind, ts,
         LAG(kind) OVER w AS prev_kind,
         LAG(ts)   OVER w AS prev_ts
  FROM events
  WHERE kind IN ('prompt.submitted', 'turn.stopped')
  WINDOW w AS (PARTITION BY session_id ORDER BY ts)
), turns AS (
  SELECT session_id, prev_ts AS t0, ts AS t1,
         (julianday(ts) - julianday(prev_ts)) * 86400.0 AS dur_s
  FROM ev
  WHERE kind = 'turn.stopped' AND prev_kind = 'prompt.submitted'
), waits AS (
  SELECT t.session_id, t.t0, t.dur_s,
         COALESCE(SUM(a.latency_ms), 0) / 1000.0 AS wait_s
  FROM turns t
  LEFT JOIN approvals a
         ON a.session_id = t.session_id
        AND a.status IN ('allowed', 'denied')
        AND a.created_at >= t.t0 AND a.created_at <= t.t1
  GROUP BY t.session_id, t.t0, t.t1
)
SELECT substr(t0, 1, 10)                                   AS day,
       COUNT(*)                                            AS turns,
       ROUND(AVG(dur_s), 1)                                AS avg_turn_s,
       ROUND(AVG(wait_s), 1)                               AS avg_wait_s,
       ROUND(AVG(MAX(dur_s - wait_s, 0)), 1)               AS avg_self_run_s
FROM waits
GROUP BY day
ORDER BY day;
