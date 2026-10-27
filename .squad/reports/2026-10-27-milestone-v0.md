# Cổng v0 (T21 · ARC-42), 2026-10-27

Kết luận: **CHƯA ĐẠT**. Chặn bởi 3 phát hiện bảo mật mức Cao ở vùng khóa (quy tắc bất biến 2), 4 task chưa chốt, popup mở chưa vào
ngân sách và chưa có số liệu dogfood. Cổng chạy trên `main` tại `195bb8a` (sau PR #24), cộng nhánh T20 (PR #25).

| # | Mục | Kết quả | Bằng chứng |
|---|---|---|---|
| 1 | Task | **CHƯA ĐẠT** | Trên `main`: T12 `done` nhưng ARC-33 đang In Review (đường nguội 456 ms); T13, T18 `review`; T16 đã merge nhưng bảng ghi `review`; T20 `review` (PR #25); T21 là chính cổng này |
| 2 | Test | ĐẠT | `cargo test --workspace` xanh (không test lỗi; 4 test e2e Claude Code thật là `#[ignore]`), `cargo clippy --workspace --all-targets -D warnings` sạch, clippy `crates/app` sạch, `pnpm -C ui test` 101/101, `tsc -b` sạch |
| 3 | Fail-safe | ĐẠT | Hub tắt: `e2e_hub_down_never_allows_and_claude_does_not_hang`, `shutdown_returns_empty_object_for_pending`. Hết giờ: `timeout_returns_empty_object_never_allow`, `http_timeout_returns_empty_object`, `e2e_timeout_returns_empty_object_and_never_allows`. Lỗi: `write_failure_never_allows`, `held_bytes_cap_returns_empty_never_allow`, `response_for_every_non_decision_is_empty_object`, `full_queue_returns_empty_object_immediately`. Rà code: `response_for` là chỗ duy nhất sinh `allow` |
| 4 | Vùng khóa | **CHƯA ĐẠT** | Cả 10 nhóm trong `danger.rs` đều có test và có test "Critical/High không bao giờ tự duyệt dù có grant hay batch" (`critical_never_auto_allowed_with_any_grant_or_batch`, `critical_ignores_grant`, `batch_rule_never_applies_to_high_or_critical`). Nhưng rà bảo mật tìm ra các dạng lách chưa có test (mục 5), đã tái hiện bằng `policy::assess` |
| 5 | Bảo mật | **CHƯA ĐẠT** | 3 phát hiện Cao, 1 Trung bình, 3 Thấp (dưới). Fail-safe, xác thực token, WebSocket/Origin, Tauri (CSP, quyền) không có lỗi đáng kể. Chưa kiểm phụ thuộc theo CVE (máy không có `cargo-audit`) |
| 6 | Ngân sách | **CHƯA ĐẠT** | Hub ĐẠT (idle 13,4 MB, đỉnh 16,7 MB, p99 0,32 ms). App: tray 16,8 MB và sau đóng popup 32,8 MB đạt; popup mở 364,5 MB WS so với ≤150 MB (vượt); mở popup 12 ms với prewarm bằng chuột thật, 456 ms đường nguội. Máy yếu 4 GB: người dùng đã bỏ qua |
| 7 | Dogfood | **CHƯA ĐẠT** | Chưa có số: baseline chưa đo; `arcturus.db` trên máy dev chỉ có 8 yêu cầu thử trong một ngày, không phải dùng hằng ngày ≥ 2 tuần (`python scripts/metrics.py`). `auto_approved_dangerous` hiện rỗng |
| 8 | Tài liệu | **CHƯA ĐẠT (nhẹ)** | `CLAUDE.md` dòng "CHƯA kiểm chứng: non-2xx, lỗi kết nối và hết giờ… (T17)" đã cũ (T17 chạy thật: allow, deny, timeout, hub tắt); `/healthz` không được tài liệu hóa; `PROTOCOL.md` chỉ mô tả phía UI. `PLAN.md:890` xếp `git push origin main`/`rm -r dir` là Medium, mâu thuẫn với `CLAUDE.md` quy tắc 2 ("lệnh mạng ra ngoài", "ghi ngoài workspace") |

## Phát hiện bảo mật (rà đọc code, đã tái hiện các ca chính bằng `hub_core::policy::assess`)

| # | Mức | Mô tả | Vị trí |
|---|---|---|---|
| 1 | **Cao** | Brace expansion (`cat .e{nv,}` → Low), tùy chọn dài viết tắt (`rm --rec --forc ~`, `git push --del`, `git reset --har` → Medium) và tham số PowerShell `-Recurse:$true` bị bộ phân tích bỏ sót nên lệnh vùng khóa tụt xuống Low/Medium; batch rule, trust grant và đếm ngược tự duyệt được | `normalize.rs:657`, `danger.rs:522-602`, `risk.rs:268-282` |
| 2 | **Cao** | Luật `arcturus_ui_token`/cấu hình/DB/`HUB_URL` chỉ so trên chữ thô: `ui-t''oken`, `local''host`, `127.1` lọt (Medium). Vô hiệu biện pháp B2 (agent không được chạm token UI) | `danger.rs:214,383-402,497-500,1128-1138` |
| 3 | **Cao** | `write_outside_workspace` chỉ xét `tee/cp/mv/install/ln/rsync/dd` và chuyển hướng: `rm -r ~`, `sed -i ~/.bashrc`, `touch`, `Remove-Item -Recurse`, `Set-Content` chỉ là Medium | `danger.rs:705-732` |
| 4 | Trung bình | "Lệnh mạng ra ngoài" thiếu `git push/fetch` tới URL lạ, `gh gist`, `npm publish`, `nslookup/dig/ping`, `Start-BitsTransfer` (Medium); cần người chốt vì `PLAN.md:890` mâu thuẫn `CLAUDE.md` | `danger.rs:937-946` |
| 5 | Thấp | `standup` chạy `git log` trong `cwd` do agent điều khiển mà không tắt kiểm chữ ký (`gpg.program` trong `.git/config`); `cwd` UNC trên Windows mở SMB | `hub/src/standup.rs:41-49` |
| 6 | Thấp | "Duyệt gộp" cộng N vào chuỗi trust cho một lần bấm | `approvals.rs:1110,1187` |
| 7 | Thấp | Route hook không kiểm `Host`; `/hooks/event` không có ngân sách byte (token hook nằm trong môi trường của agent) | `hooks.rs:21-32,127-135` |

## Việc cần làm (đề xuất, chưa thêm vào `.squad/TASKS.md`, chưa tạo ticket)

1. **Policy, ưu tiên cao:** đóng 3 ca Cao: brace expansion coi là chưa phân tích được (High); tùy chọn dài so theo tiền tố; bỏ `:<giá trị>` của tham số PowerShell; chạy needles và `HUB_URL` trên từ đã bỏ nháy; phân tích host bằng `std::net` (kể cả `127.1`, hex, `::ffff:`); mở rộng đích ghi cho `rm/rmdir/sed -i/touch/mkdir/chmod/Remove-Item/Set-Content…`. Test hồi quy cho mọi ca trong bảng (làn `policy`, TDD).
2. **Người quyết định:** `git push`/`rm -r` và "lệnh mạng ra ngoài": theo `CLAUDE.md` (khóa) hay `PLAN.md:890` (Medium); sau đó đồng bộ tài liệu.
3. **Hub:** phát hiện 5–7 (`--no-show-signature`, bỏ `cwd` không cục bộ, kiểm `Host` và ngân sách byte cho route hook, mỗi lần duyệt gộp chỉ cộng một lần chuỗi trust).
4. **Ngân sách app:** popup mở ≤150 MB và mở nguội ≤300 ms (WebView2): cần quyết định giữ ngưỡng hay chỉnh (`docs/BUDGET.md`).
5. **Dogfood:** người dùng đo baseline tay (`docs/DOGFOOD.md` mục 10) rồi dùng hằng ngày ≥ 2 tuần; chạy lại cổng.
6. **Tài liệu:** sửa dòng "CHƯA kiểm chứng" trong `CLAUDE.md` (chỉ còn agent teams Q9), thêm `/healthz` và mô tả route hook vào `PROTOCOL.md`/`ARCHITECTURE.md`.
7. **CI:** thêm `cargo audit` (hoặc `cargo deny`) cho cả hai workspace và `pnpm audit`; chạy kiểm phụ thuộc theo CVE chưa làm được.
8. Chốt trạng thái: T12 (ARC-33, sau quyết định đường nguội), T13, T18, T16, T20.

## Cập nhật sau báo cáo (cùng ngày)

Phát hiện 1–3 (mức Cao) đã sửa, kèm test hồi quy, ở nhánh `arc-29-danger-evasions` (PR riêng). Đã chạy lại `policy::assess` với các ca trong
bảng: `rm --rec --forc ~`, `git push --del/--mirr`, `git reset --har`, `git clean --forc`, `Remove-Item -Recurse:$true -Force:$true`,
`ui-t''oken`, `local''host`, `127.1`, `sed -i … ~/.bashrc`, `touch ~/.bashrc` → Critical; `cat .e{nv,}` → High (không phân tích được).
Còn mở: phát hiện 4 (`git push`/`rm -r`/"lệnh mạng ra ngoài": người dùng quyết định), 5–7, mục 1, 6, 7, 8 của cổng.
