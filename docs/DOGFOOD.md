# Dogfood: dùng Arcturus với Claude Code thật

Mục tiêu của T18 · ARC-39: bạn dùng Arcturus hằng ngày để duyệt yêu cầu của Claude Code, rồi ghi lại chỗ nào khó chịu
(`.squad/TASKS.md`, mục "Dogfood"). Số liệu của T20 (thời gian xử lý mỗi yêu cầu trước/sau) và cổng v0 (T21) dựa trên
việc dùng thật này.

**An toàn khi thử:** hub chết, tắt, hết giờ hay sai token đều là lỗi không chặn: Claude Code hỏi trong terminal như bình
thường. Terminal luôn hỏi song song với popup, nên bạn vẫn trả lời được ở terminal. Hub không bao giờ tự `allow` lệnh
thuộc vùng khóa (`rm -rf`, `git push --force`, `curl | sh`, đọc `.env`/khóa, ghi ngoài workspace, mạng ra ngoài…).

## 0. Cách nhanh: cài trong app (khuyên dùng)

Không cần `jq`, `bash` hay đặt `ARCTURUS_TOKEN` bằng tay:

1. Chạy app (bước 3). Lần đầu, nếu chưa cài hook, app tự mở cửa sổ **Cài đặt**; lần sau mở bằng menu tray
   **Cài đặt và tự kiểm tra…**.
2. **Xem trước khi cài**: app hiện diff của `settings.json` (token được che). Bấm **Đồng ý cài**: app sao lưu file
   bên cạnh rồi chỉ sửa khóa `hooks`, đúng như `dogfood-install.sh` (hook của bạn giữ nguyên, chạy lại không nhân đôi,
   giữ symlink và quyền file). File đổi giữa lúc xem trước và lúc bấm thì app không ghi, yêu cầu xem trước lại.
3. Mở một phiên `claude` mới.
4. **Chạy tự kiểm tra**: app gửi một sự kiện thử (`UserPromptSubmit` của phiên `arcturus-selftest-…`) qua đúng URL và
   token của hook đã cài, rồi báo từng bước: hook có trong settings.json, hub đang chạy, hook tới được hub (401 = token
   lệch, lỗi kết nối = sai cổng), giao diện thấy sự kiện qua WebSocket. Không tạo yêu cầu duyệt; agent thử biến mất ngay
   sau đó (nó vẫn được ghi vào bảng `events` như mọi sự kiện).
5. Gỡ: nút **Gỡ hook…**, cũng qua bước xem trước.

Token hook: app dùng `ARCTURUS_TOKEN` nếu bạn đã đặt (tương thích cách cũ), nếu không thì tự sinh và lưu ở
`<data_dir>/arcturus/hook-token` (Unix: quyền 0600), đưa cho hub con và ghi thẳng vào hook trong `settings.json`. Token
không còn nằm trong biến môi trường của `claude`. Bản cài bằng script (`$ARCTURUS_TOKEN`) được cửa sổ Cài đặt nhận ra
và đề nghị cài lại. Các mục 1–4 dưới đây là cách cài bằng script, vẫn dùng được.

## 1. Chuẩn bị

| Cần | Ghi chú |
|---|---|
| Claude Code | đã chạy ít nhất một lần (có thư mục `~/.claude`) |
| `jq`, `bash` | cho script cài/gỡ. Windows: Git Bash + `winget install jqlang.jq`. Linux: `apt install jq`. macOS: `brew install jq` |
| `curl` | hook `SessionStart` gọi `curl` (Windows 10+ có sẵn `curl.exe`) |
| Hub và app đã build | `cargo build -p hub` rồi chạy app ở bước 3 (trong repo này, app tìm hub ở `target/debug/hub`) |
| Token | bước 2 |

## 2. Tạo token và đặt biến môi trường

Hook xác thực với hub bằng biến `ARCTURUS_TOKEN` (≥ 32 ký tự, ≥ 10 ký tự khác nhau). **Cùng một giá trị** phải có ở nơi
khởi động app/hub **và** ở nơi khởi động `claude`.

```bash
# bash / zsh / Git Bash
openssl rand -hex 24                     # in ra token; chép vào ~/.bashrc (hoặc ~/.zshrc):
export ARCTURUS_TOKEN=<token vừa tạo>
```

```powershell
# PowerShell (đặt vĩnh viễn cho người dùng; mở terminal mới sau đó)
$t = -join ((1..24) | ForEach-Object { '{0:x2}' -f (Get-Random -Maximum 256) })
[Environment]::SetEnvironmentVariable('ARCTURUS_TOKEN', $t, 'User')
```

Đây là token của **hook**. Token của UI là chuyện khác: app tự khởi động hub và nhận token UI qua pipe, không có file
token trên đĩa (docs/PLAN.md §3.3).

## 3. Chạy app

```bash
cargo build -p hub
pnpm -C crates/app install
pnpm -C crates/app tauri dev
```

App chỉ hiện icon ở khay hệ thống. Nó tự khởi động hub (`127.0.0.1:7878`) với `ARCTURUS_TOKEN` thừa hưởng từ môi trường
của nó. Nếu không thấy hub, tooltip của icon ghi dòng lỗi cuối của hub (thường là thiếu hoặc sai `ARCTURUS_TOKEN`).

- Bấm trái vào icon (Windows, macOS) hoặc menu **Mở hàng đợi duyệt** (mọi nền tảng) để mở popup.
- Đổi cổng: đặt `[hub] bind` trong `arcturus.toml`, `ARCTURUS_HUB_ADDR` cho app, và cài hook bằng `--port N` (bước 4).
- Persona (tên, màu nhân vật theo thư mục): sao chép `arcturus.example.toml` thành
  `<thư mục cấu hình>/arcturus/arcturus.toml` (Linux `~/.config`, macOS `~/Library/Application Support`, Windows
  `%APPDATA%`) rồi chạy hub với `--config` đó (app dùng đường dẫn mặc định).

## 4. Cài hook

```bash
./scripts/dogfood-install.sh --dry-run    # xem trước thay đổi, không ghi gì
./scripts/dogfood-install.sh              # gộp vào ~/.claude/settings.json (hoặc $CLAUDE_CONFIG_DIR/settings.json)
```

Script:
- Chỉ đụng khóa `hooks`. Hook của bạn giữ nguyên; hook của Arcturus được thêm vào cuối mảng của từng sự kiện.
- Chạy lại nhiều lần an toàn (không nhân đôi).
- Sao lưu `settings.json.bak-<giờ>` bên cạnh file trước khi ghi. JSON hỏng thì dừng, không ghi gì.
- Giữ symlink và quyền file (nhiều người để `settings.json` trong repo dotfiles).
- `--settings FILE` để cài vào một file khác, ví dụ `.claude/settings.local.json` của riêng một dự án.

**Mở một phiên Claude Code mới** sau khi cài: hook được đọc lúc phiên bắt đầu.

Hook đăng ký (`hooks/arcturus-dogfood.json`): `PermissionRequest` (giữ yêu cầu tới 120 s; hub tự trả `{}` ở 110 s),
`SessionStart` (hook `command` gọi `curl`, vì hook `http` không chạy cho `SessionStart`: `docs/HOOK_FINDINGS.md` F1), và
các sự kiện còn lại (`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `Notification`,
`SubagentStart`, `SubagentStop`, `Stop`, `StopFailure`, `SessionEnd`) gửi tới `/hooks/event` với timeout 2–3 s.

## 5. Kiểm tra đã nối đúng

1. `curl http://127.0.0.1:7878/healthz` trả `ok`.
2. Mở `claude --permission-mode default` trong một thư mục thử, bảo nó: *"chạy lệnh `touch dogfood-marker.txt`"*.
3. Trong vài giây: icon tray có chấm, popup có thẻ với lệnh `touch dogfood-marker.txt`, rủi ro "Rủi ro vừa".
4. Chờ nút **Đồng ý** hết mờ (1 s), bấm Enter: lệnh chạy, terminal không cần trả lời. Hoặc bấm Esc: lệnh bị từ chối và
   Claude Code nhận lời từ chối.
5. Thử một lệnh vùng khóa, ví dụ *"chạy `rm -rf dogfood-thu-muc`"* (thư mục không tồn tại): thẻ hiện nhãn **Vùng khóa**
   và tô sáng `rm -rf`; hub không tự duyệt.
6. Sự kiện được ghi vào SQLite. Xem nhật ký duyệt (Linux; macOS/Windows đổi đường dẫn theo `dirs::data_dir()`):
   ```bash
   sqlite3 ~/.local/share/arcturus/arcturus.db \
     "select tool, risk, status, decided_by, latency_ms from approvals order by created_at desc limit 10"
   ```

## 6. Kiểm tra hub tắt vẫn an toàn

Làm một lần ngay sau khi cài, để biết chắc chắn không bị khóa.

> **Lưu ý:** theo tài liệu của Claude Code, hook lỗi kết nối, non-2xx hoặc hết giờ là lỗi không chặn, nhưng trường hợp
> này **chưa được kiểm chứng trên máy thật** (T17 · ARC-38 sẽ tự động hóa; `docs/HOOK_FINDINGS.md`). Bước 1 dưới đây chính
> là phép thử đó. Nếu terminal bị treo hoặc lệnh bị từ chối khi hub tắt, **gỡ hook ngay** (bước 7) và báo lại phiên bản
> `claude --version`. Riêng hook `SessionStart` đã được thử: hub tắt, sai hay thiếu token thì lệnh `curl` im lặng,
> thoát 0 trong 0,01 s (không in gì vào ngữ cảnh của Claude).

1. **Hub tắt hẳn.** Menu tray → **Thoát** (hub con tắt theo). Chạy lại một lệnh cần duyệt trong `claude`: terminal phải hỏi
   như chưa cài Arcturus, không treo, không trễ đáng kể.
2. **Hub đang giữ yêu cầu thì tắt.** Gửi một yêu cầu cần duyệt, **đừng bấm gì ở popup**, thoát app. Hub trả `{}` cho yêu
   cầu đang giữ, hộp thoại ở terminal vẫn đó và chờ bạn.
3. **Không ai ở popup.** Để yêu cầu quá 110 s: hub trả `{}`, hộp thoại ở terminal vẫn chờ (ở chế độ headless `-p` thì lệnh
   bị từ chối, không bao giờ được allow).
4. **Trả lời ở terminal trước.** Bấm `1` (Yes) ở terminal khi popup còn thẻ: lệnh chạy, thẻ biến mất sau khoảng 1–2 s
   (hub nhận `PostToolUse` cùng lệnh và dọn thẻ).

## 7. Gỡ

```bash
./scripts/dogfood-uninstall.sh --dry-run
./scripts/dogfood-uninstall.sh
```

Chỉ gỡ hook trỏ tới hub của Arcturus (`http://127.0.0.1:<cổng>/hooks/event|permission`); hook của bạn và mọi khóa khác
giữ nguyên. Mục hoặc khóa trở nên rỗng vì việc gỡ thì bị bỏ. Không có gì để gỡ thì không ghi, không sao lưu. Phiên
`claude` đang chạy giữ hook cũ tới khi đóng. Khôi phục tay: chép lại `settings.json.bak-<giờ>` mong muốn.

## 8. Giới hạn đã biết

- **Chỉ thấy yêu cầu thật sự hỏi người.** Lệnh bạn đã cho phép sẵn trong settings, lệnh chỉ đọc (ví dụ `hostname`) và
  chế độ `auto` không sinh `PermissionRequest`, nên Arcturus không thấy (F2).
- **Lệnh quá dài** (mặc định > 4.096 byte hoặc > 50 dòng, trừ Edit/Write) hub không đưa lên thẻ: nó trả `{}` ngay và thẻ
  báo "Duyệt trong terminal" để bạn xem đủ lệnh ở terminal (quy tắc bất biến 3).
- **Token của hook đọc được bởi agent** (F6): Bash tool thừa hưởng biến môi trường của `claude`, kể cả `ARCTURUS_TOKEN`.
  Hệ quả: agent có thể gửi sự kiện giả tới `/hooks/event` (chỉ làm sai hiển thị trạng thái, không thể duyệt gì). Việc
  duyệt dùng token UI riêng mà agent không có, và mọi lệnh chạm token/hub đều bị xếp vào vùng khóa. Chi tiết:
  `docs/ARCHITECTURE.md` "Giới hạn bảo mật đã biết".
- **"Tạm dừng cả đội" không dừng được agent thật** ở v0: chỉ tắt mọi tự duyệt và hiện trạng thái tạm dừng.
- **Hook không vượt được deny rule** trong settings của Claude Code: lệnh bị deny rule chặn sẽ không bao giờ tới Arcturus.
- **Nhật ký SQLite chứa lệnh và đường dẫn** bạn đã duyệt (`approvals.command_original`) và JSON hook gốc của sự kiện
  (`events.raw_json`, tối đa 256 KiB mỗi sự kiện). Unix: file chỉ người dùng đọc được (0600). Windows: dựa vào quyền mặc
  định của thư mục hồ sơ. Sự kiện tự xóa sau `event_ttl_days` (30 ngày); bảng `approvals` không tự xóa ở v0: xóa file
  `arcturus.db*` (khi hub đã tắt) nếu muốn bắt đầu lại.
- Dogfood trên máy bạn: hub chỉ nghe `127.0.0.1`, không có gì ra ngoài máy.

## 9. Ghi lại khi dùng

- Chỗ khó chịu (thẻ khó đọc, phím tắt, thông báo trùng, popup chậm…): thêm một dòng vào `.squad/TASKS.md`, mục "Dogfood".
- Thấy `ARCTURUS_TOKEN` hay hook gây lỗi lạ: ghi lại phiên bản Claude Code (`claude --version`) và dòng log của hub.
- Hub log ra stdout (hub chạy riêng) hoặc stderr của app (`[hub] …`); không bao giờ có token hay nội dung lệnh ở mức `info`.
- Số đo cho T20 (thời gian xử lý mỗi yêu cầu, thời gian agent tự chạy) lấy từ bảng `approvals` (`latency_ms`) và `events`.

## 10. Chỉ số của T20 (trước/sau Arcturus)

Cổng v0 (`/milestone-gate v0`, mục Dogfood) đòi: dùng hằng ngày ≥ 2 tuần, **thời gian xử lý trung bình mỗi yêu cầu giảm ≥ 30%
so với baseline**, **0 lần tự duyệt sai**. Các con số "sau" tính từ nhật ký SQLite của hub bằng `scripts/metrics.sql`; baseline
(trước Arcturus) phải đo tay vì khi đó không có hub.

### Lấy số "sau"

```bash
sqlite3 -header -column "<đường dẫn arcturus.db>" < scripts/metrics.sql   # máy có sqlite3
python scripts/metrics.py                                                 # Windows/không có sqlite3 (mở CHỈ ĐỌC)
```

Đường dẫn mặc định của hub: Windows `%APPDATA%\arcturus\arcturus.db`, macOS `~/Library/Application Support/arcturus/arcturus.db`,
Linux `~/.local/share/arcturus/arcturus.db` (hub in `db=...` ở dòng log đầu tiên). Năm truy vấn:

| Truy vấn | Đọc gì | Dùng cho |
|---|---|---|
| `per_day` | mỗi ngày: số yêu cầu, chờ trung bình (`avg_wait_user_s` là phần người duyệt), `policy_pct`, deny, timeout | xu hướng, "dùng hằng ngày" (số ngày có dữ liệu) |
| `total` | một dòng cho cả khoảng dữ liệu | so với baseline |
| `auto_approved_risk` | số lần policy tự allow theo mức rủi ro | kiểm tự duyệt chỉ ở mức thấp |
| `auto_approved_dangerous` | policy đã tự allow yêu cầu `high`/`critical` | **phải rỗng** (0 lần tự duyệt sai) |
| `agent_self_run` | mỗi lượt (prompt → trả lời xong): thời gian cả lượt, tổng chờ duyệt, thời gian agent tự chạy | "thời gian agent tự chạy" |

Chờ = từ lúc yêu cầu vào hub tới lúc có quyết định (`latency_ms`). Yêu cầu hết giờ, bị hủy và "trả về terminal" không có người
quyết nên không đưa vào trung bình (đếm riêng). Yêu cầu chạy song song cộng dồn thời gian chờ, nên `avg_self_run_s` kẹp ở 0.

### Đo baseline (làm tay, trước khi cài hook)

1. Dùng Claude Code như bình thường, **chưa** chạy `dogfood-install.sh`.
2. Mỗi lần hộp thoại xin quyền hiện ở terminal, bấm giờ từ lúc nó hiện tới lúc bạn trả lời (đồng hồ điện thoại hoặc ghi giờ
   hai lần). Ghi ≥ 30 yêu cầu, ít nhất ở 3 ngày khác nhau.
3. Điền trung bình giây/yêu cầu vào bảng dưới. Mục tiêu: "sau" ≤ 70% của baseline.

### Bảng kết quả

| | Khoảng ngày | Số yêu cầu | Chờ trung bình mỗi yêu cầu (giây) | Tự duyệt sai | Ghi chú |
|---|---|---|---|---|---|
| Baseline (trước Arcturus, đo tay) | _chưa đo_ | | | — | cần người dùng đo theo 3 bước trên |
| Sau (Arcturus, `avg_wait_user_s`) | _chưa có dữ liệu dogfood_ | | | | |

Trạng thái 2026-10-27 (máy phát triển): `arcturus.db` mặc định có 8 yêu cầu trong một ngày (2026-10-26), do thử T12/T14 chứ không phải
dùng hằng ngày; 2 yêu cầu có người quyết định (trung bình 18 giây), 0 yêu cầu do policy tự duyệt, `auto_approved_dangerous` rỗng,
chưa có lượt (prompt → trả lời) nào để tính `agent_self_run`. Không dùng các số này làm baseline hay kết quả dogfood.
