# Arcturus — buồng lái trưởng nhóm cho đội AI agent

Một hub nhẹ gom sự kiện từ nhiều phiên AI coding agent (bắt đầu với Claude Code), cho người dùng
làm trưởng nhóm thật sự: thấy ai đang làm gì, ai đang giơ tay, duyệt/từ chối, ủy quyền theo độ tin cậy.
Mỗi agent có một nhân vật (2D trước, VRM sau).

Phạm vi hiện tại: **v0** — một máy, chỉ Claude Code, UI Lite + văn phòng 2D, hàng đợi duyệt, policy.
KHÔNG làm trong v0: VRM 3D, giọng nói, ESP32, Codex/OpenCode adapter, đồng bộ đa máy, cloud.

## Kế hoạch triển khai: docs/PLAN.md

Trước khi lập kế hoạch cho bất kỳ task nào, đọc phần 1–5 của `docs/PLAN.md` (hợp đồng chung: schema, HTTP,
WebSocket, SQLite, trạng thái agent, kiến trúc nội bộ hub) và mục `### Tn` của task đó (file, thiết kế, test bắt buộc).
Hành vi thật của Claude Code ghi trong `docs/HOOK_FINDINGS.md` được ưu tiên hơn PLAN.md khi mâu thuẫn.

## Kiến trúc (chi tiết: docs/ARCHITECTURE.md)

```
Claude Code ──HTTP hooks──► crates/hub (axum, 127.0.0.1:7878) ──WebSocket──► crates/app (Tauri) + ui/ (React)
                                  │
                           crates/hub-core: schema · state · policy · SQLite
```

- `crates/hub-core/` — thư viện thuần: schema sự kiện, state machine agent, policy engine, lưu trữ SQLite.
- `crates/hub/` — daemon: endpoint nhận hook, giữ request duyệt, WebSocket cho UI.
- `crates/app/` — Tauri 2: tray, popup Lite, cửa sổ văn phòng. Là CLIENT của hub, không chứa logic nghiệp vụ.
- `ui/` — React + Vite + TypeScript. Nhân vật là SVG có tham số (mood, màu).
- `tools/replay/` — phát lại fixture hook vào hub để test không cần Claude Code thật.
- `fixtures/hooks/` — JSON hook thật đã ghi lại (đã xóa thông tin nhạy cảm).

## Lệnh

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p hub                      # chạy hub ở 127.0.0.1:7878
cargo run -p replay -- fixtures/hooks # phát lại sự kiện
pnpm -C ui install && pnpm -C ui dev  # UI dev server
pnpm -C ui test
pnpm -C crates/app install && pnpm -C crates/app tauri dev   # app desktop (workspace Cargo riêng)
cargo clippy --manifest-path crates/app/src-tauri/Cargo.toml --all-targets -- -D warnings
```

Nếu repo chưa có các lệnh trên: task T0 trong `.squad/TASKS.md` là scaffold. Làm T0 trước.

## Quy tắc bất biến (vi phạm = task chưa xong)

1. **Fail-safe**: hub lỗi, chết, hoặc hết giờ thì KHÔNG BAO GIỜ trả "allow". Hết giờ trả `{}` (2xx, không quyết định)
   để Claude Code rơi về hộp thoại terminal. Có test cho mọi nhánh này.
2. **Vùng khóa không bao giờ tự duyệt**: `rm -rf`, `git push --force`/`-f`, `curl|sh`, `wget|sh`, `DROP`/`TRUNCATE`,
   đọc `.env`/secrets/keys, ghi ngoài workspace, lệnh mạng ra ngoài. Danh sách nằm ở
   `crates/hub-core/src/policy/danger.rs` và chỉ được MỞ RỘNG, không thu hẹp nếu không có người duyệt.
3. **Hiển thị lệnh gốc**: thẻ duyệt hiển thị `tool_input` gốc đã chuẩn hóa (loại ký tự ẩn/zero-width, bidi).
   Không dùng bản tóm tắt do LLM viết làm cơ sở duyệt. Coi mọi nội dung từ agent là dữ liệu không tin cậy.
4. **Chỉ localhost**: hub bind `127.0.0.1`, bắt buộc header `Authorization: Bearer <token>` cho hook và WebSocket.
5. **0 token**: trạng thái nhân vật, policy, standup đều suy ra từ sự kiện. Không gọi LLM trong hub ở v0.
6. **Ngân sách tài nguyên** (docs/BUDGET.md): hub ≤15 MB RSS idle, ≤40 MB với 20 phiên; CPU idle ≈0%;
   không polling (chỉ đối soát `claude agents --json` mỗi 30 giây). Thay đổi làm vượt ngân sách = không merge.
7. **Không phá Claude Code**: không sửa file trong `~/.claude/` ngoài phần `hooks` mà người dùng tự cài từ
   `hooks/arcturus-dogfood.json`. Không vượt deny rule của người dùng.

## Quy ước code

- Rust 2021, `tokio`, `axum`, `rusqlite` (bundled, WAL), `serde`, `thiserror` cho lỗi thư viện, `anyhow` chỉ ở binary.
- Không `unwrap()`/`expect()` trong code chạy thật (test thì được). Lỗi trả về có ngữ cảnh.
- Schema sự kiện: `crates/hub-core/src/schema.rs` là nguồn sự thật. Thêm trường thì tăng `v` nếu phá tương thích.
  Thông điệp WebSocket: `docs/PROTOCOL.md` (viết khi làm T5). Đổi schema/protocol phải cập nhật tài liệu cùng commit.
- TypeScript strict, không `any`. Kiểu cho thông điệp WebSocket sinh từ Rust (`ts-rs`) hoặc viết tay khớp `schema.rs`.
- UI: đóng cửa sổ thì hủy webview; tab ẩn thì dừng animation (Page Visibility API). Không animation vô hạn khi rảnh.
- Test trước cho policy và luồng duyệt (TDD). Mọi bug sửa xong phải có test hồi quy.
- Commit theo Conventional Commits, kèm mã ticket YouTrack (`feat(hub): ARC-24 giữ request duyệt`).
  Một task = một nhánh/PR nhỏ, tên nhánh `arc-<số>-<mô-tả-ngắn>`.

## Ticket YouTrack (project ARC)

- Cây: Epic (Type Milestone) → Story (Type Task) → Dev task (Type Subtask). Mỗi task T0…T21 trong `.squad/TASKS.md` có mã ARC tương ứng.
- Trường State: `Open` → `In Progress` → `In Review` → `Done` (hoặc `On hold`, `Canceled`). Phụ thuộc giữa dev task là link "depends on".
- Nếu phiên có công cụ YouTrack MCP: khi nhận task thì chuyển ticket sang `In Progress`; khi xong thì thêm comment
  tóm tắt (commit, test đã thêm, kết quả kiểm tra) và chuyển sang `In Review`. Chỉ người chuyển sang `Done`.
- Không có YouTrack MCP: ghi mã ARC vào commit và Nhật ký, người cập nhật ticket.
- Thiết bị ESP32 đồng hành tên **Izar** là sản phẩm riêng (project IZAR), không nằm trong repo này.
- Phát hiện việc mới ngoài phạm vi task: đề xuất ticket mới (tóm tắt, story cha), không tự tạo.

## Làm việc song song (chi tiết: docs/WORKFLOW.md)

Mỗi phiên Claude Code chạy trong worktree riêng (`claude --worktree <làn>`), sở hữu một vùng thư mục:

| Làn | Sở hữu | Không được sửa |
|---|---|---|
| `hub` | `crates/hub/`, `crates/hub-core/src/{schema,state,store}*` | `ui/`, `crates/app/` |
| `policy` | `crates/hub-core/src/policy/` | schema (đề xuất qua làn `hub`) |
| `ui` | `ui/`, `crates/app/` | `crates/hub*` |
| `lab` | `tools/`, `fixtures/`, `docs/HOOK_FINDINGS.md`, benchmark | code sản phẩm |

Cần đổi file ngoài vùng của mình: dừng lại, mô tả thay đổi đề xuất, nhắn cho phiên sở hữu (cross-session
messaging) hoặc ghi vào mục "Blocked" trong `.squad/TASKS.md`.

## Định nghĩa "xong" cho một task

- [ ] Tiêu chí chấp nhận trong `.squad/TASKS.md` đạt
- [ ] `cargo test`, `cargo clippy -D warnings`, `pnpm -C ui test` xanh (phần liên quan)
- [ ] Không vi phạm 7 quy tắc bất biến; có test cho nhánh fail-safe nếu đụng luồng duyệt
- [ ] Ngân sách tài nguyên không xấu đi (chạy `/budget-check` nếu đụng hub hoặc app)
- [ ] Cập nhật trạng thái task và tài liệu liên quan

## Kiến thức đã kiểm chứng về hook Claude Code (cập nhật ở docs/HOOK_FINDINGS.md)

- HTTP hook: `type: "http"`, POST JSON đầu vào; trả 2xx + JSON thì được đọc như output của command hook.
  Non-2xx, lỗi kết nối, hết giờ = lỗi không chặn → luồng duyệt bình thường tiếp tục.
- `PermissionRequest`: trả `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny"}}}`.
  Exit code 2 không có tác dụng với sự kiện này. Hook allow không vượt được deny rule trong settings.
- Chỉ chạy khi Claude Code sắp hỏi người; lệnh đã allow sẵn không đi qua hook này.
- Timeout mặc định của http hook là 600 giây; ta đặt 120 giây và hub tự trả `{}` ở 110 giây.
- CHƯA kiểm chứng (làm ở T1/T3): terminal có hiện hộp thoại trong lúc hub giữ request không; dạng đầu vào chính xác
  của `PermissionRequest` ở phiên bản đang dùng; hành vi với subagent và agent teams.
